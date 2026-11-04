//! Type checking for constraint and policy expressions (SVT003).

use crate::expr::{self, Expr, Op};
use crate::model::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    Num,
    Text,
    Bool,
    Id,
    Time,
    Json,
    Null,
}

impl Ty {
    fn name(self) -> &'static str {
        match self {
            Ty::Num => "a number",
            Ty::Text => "text",
            Ty::Bool => "true/false",
            Ty::Id => "an id",
            Ty::Time => "a time",
            Ty::Json => "json",
            Ty::Null => "null",
        }
    }
}

fn field_ty(t: &FieldType) -> Ty {
    match t {
        FieldType::Text | FieldType::Enum(_) => Ty::Text,
        FieldType::Int | FieldType::Float | FieldType::Decimal { .. } | FieldType::Money { .. } => {
            Ty::Num
        }
        FieldType::Bool => Ty::Bool,
        FieldType::Uuid | FieldType::Ref(_) | FieldType::User => Ty::Id,
        FieldType::Timestamp | FieldType::Date => Ty::Time,
        FieldType::Json => Ty::Json,
    }
}

/// Where an expression appears decides which paths it may use (docs/format.md, Expressions).
pub struct Scope<'a> {
    pub entity: &'a Entity,
    pub entities: &'a HashMap<&'a str, &'a Entity>,
    /// Policies may read one reference away and use `auth.user`; constraints may not.
    pub policy: bool,
    pub roles: &'a [RoleDef],
    /// Set when the error came from `has_role`, so it's reported as SVT006 rather than SVT003.
    role_error: std::cell::Cell<bool>,
}

impl<'a> Scope<'a> {
    pub fn new(
        entity: &'a Entity,
        entities: &'a HashMap<&'a str, &'a Entity>,
        policy: bool,
        roles: &'a [RoleDef],
    ) -> Self {
        Scope {
            entity,
            entities,
            policy,
            roles,
            role_error: std::cell::Cell::new(false),
        }
    }
}

/// Why an expression was rejected: a role problem (SVT006) or any other problem (SVT003).
#[derive(Debug, PartialEq)]
pub enum Problem {
    Role(String),
    Expr(String),
}

impl Problem {
    pub fn split(self) -> (crate::diag::Code, String) {
        match self {
            Problem::Role(m) => (crate::diag::Code::Svt006, m),
            Problem::Expr(m) => (crate::diag::Code::Svt003, m),
        }
    }
}

/// Check that `src` parses and is a boolean expression in `scope`. The error is a full sentence.
pub fn check(src: &str, scope: &Scope) -> Result<(), Problem> {
    check_str(src, scope).map_err(|m| {
        if scope.role_error.get() {
            Problem::Role(m)
        } else {
            Problem::Expr(m)
        }
    })
}

fn check_str(src: &str, scope: &Scope) -> Result<(), String> {
    let e = expr::parse(src).map_err(|p| {
        format!(
            "Can't parse `{src}` at character {}: {}.",
            p.at + 1,
            p.message
        )
    })?;
    match scope.ty(&e)? {
        Ty::Bool => Ok(()),
        other => Err(format!(
            "`{src}` evaluates to {}, but it must be true or false.",
            other.name()
        )),
    }
}

impl Scope<'_> {
    fn field<'e>(&self, entity: &'e Entity, name: &str) -> Result<&'e Field, String> {
        entity
            .fields
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| format!("{} has no field `{name}`.", entity.name))
    }

    fn path(&self, path: &[String]) -> Result<(Ty, Option<&[String]>), String> {
        match path {
            [f] => {
                let field = self.field(self.entity, f)?;
                let values = match &field.ty {
                    FieldType::Enum(v) => Some(v.as_slice()),
                    _ => None,
                };
                Ok((field_ty(&field.ty), values))
            }
            [a, b] if a == "auth" && b == "user" => {
                if self.policy {
                    Ok((Ty::Id, None))
                } else {
                    Err("`auth.user` can only be used in policies.".into())
                }
            }
            [a, b] => {
                if !self.policy {
                    return Err(format!(
                        "Constraints can only use fields of their own entity, not `{a}.{b}`; a SQL check can't read other tables."
                    ));
                }
                let FieldType::Ref(target) = &self.field(self.entity, a)?.ty else {
                    return Err(format!(
                        "`{a}` is not a reference, so `{a}.{b}` doesn't exist."
                    ));
                };
                let target = self
                    .entities
                    .get(target.name.as_str())
                    .ok_or_else(|| format!("No entity named `{}`.", target.name))?;
                Ok((field_ty(&self.field(target, b)?.ty), None))
            }
            _ => Err(format!(
                "`{}` goes more than one reference deep; only one hop is allowed.",
                path.join(".")
            )),
        }
    }

    fn ty(&self, e: &Expr) -> Result<Ty, String> {
        Ok(match e {
            Expr::Num(_) => Ty::Num,
            Expr::Str(_) => Ty::Text,
            Expr::Bool(_) => Ty::Bool,
            Expr::Null => Ty::Null,
            Expr::Path(p) => self.path(p)?.0,
            Expr::Call(name, args) => self.call(name, args)?,
            Expr::Neg(x) => self.expect(x, Ty::Num, "`-`")?,
            Expr::Not(x) => self.expect(x, Ty::Bool, "`not`")?,
            Expr::Bin(Op::And | Op::Or, l, r) => {
                self.expect(l, Ty::Bool, "`and`/`or`")?;
                self.expect(r, Ty::Bool, "`and`/`or`")?
            }
            Expr::Bin(Op::Add | Op::Sub | Op::Mul | Op::Div, l, r) => {
                self.expect(l, Ty::Num, "arithmetic")?;
                self.expect(r, Ty::Num, "arithmetic")?
            }
            Expr::Bin(op, l, r) => {
                let (lt, rt) = (self.ty(l)?, self.ty(r)?);
                self.enum_literal(l, r)?;
                self.enum_literal(r, l)?;
                let ordered = matches!(op, Op::Lt | Op::Le | Op::Gt | Op::Ge);
                let ok = if ordered {
                    matches!(
                        (lt, rt),
                        (Ty::Num, Ty::Num)
                            | (Ty::Time, Ty::Time)
                            | (Ty::Time, Ty::Text)
                            | (Ty::Text, Ty::Time)
                    )
                } else {
                    lt == rt
                        || lt == Ty::Null
                        || rt == Ty::Null
                        || matches!((lt, rt), (Ty::Time, Ty::Text) | (Ty::Text, Ty::Time))
                };
                if !ok {
                    return Err(format!("Can't compare {} with {}.", lt.name(), rt.name()));
                }
                Ty::Bool
            }
            Expr::In(x, items) => {
                let xt = self.ty(x)?;
                for item in items {
                    let it = self.ty(item)?;
                    if it != xt {
                        return Err(format!("`in` list mixes {} with {}.", xt.name(), it.name()));
                    }
                    self.enum_literal(x, item)?;
                }
                Ty::Bool
            }
        })
    }

    /// `has_role('admin')` for a global role, or `has_role('staff', location)` for a scoped one,
    /// where `location` is a ref to the role's scope entity, or `id` on the scope entity itself.
    fn call(&self, name: &str, args: &[Expr]) -> Result<Ty, String> {
        if name != "has_role" {
            return Err(format!(
                "Unknown function `{name}`; the only function is has_role."
            ));
        }
        if !self.policy {
            return Err("`has_role` can only be used in policies.".into());
        }
        let role_err = |m: String| {
            self.role_error.set(true);
            Err(m)
        };
        let Some(Expr::Str(role)) = args.first() else {
            return role_err(
                "has_role needs a role name in quotes, like has_role('admin').".into(),
            );
        };
        let Some(def) = self.roles.iter().find(|r| &r.name == role) else {
            return role_err(format!("No role named `{role}`. Declare it under `roles`."));
        };
        match (&def.scope, &args[1..]) {
            (None, []) => Ok(Ty::Bool),
            (None, _) => role_err(format!(
                "`{role}` is a global role, so has_role takes only its name: has_role('{role}')."
            )),
            (Some(s), [Expr::Path(p)]) if p.len() == 1 => {
                let f = &p[0];
                let points_at_scope = self.entity.fields.iter().any(|x| {
                    &x.name == f
                        && (matches!(&x.ty, FieldType::Ref(r) if r.name == s.name)
                            || (x.primary && self.entity.name == s.name))
                });
                if points_at_scope {
                    Ok(Ty::Bool)
                } else {
                    role_err(format!(
                        "`{role}` is held per {}, so its second argument must be a link to a {}, or `id` in a policy on {} itself.",
                        s.name, s.name, s.name
                    ))
                }
            }
            (Some(s), _) => role_err(format!(
                "`{role}` is held per {}, so has_role needs a second argument pointing at one, like has_role('{role}', {}).",
                s.name,
                crate::compile::names::snake(&s.name)
            )),
        }
    }

    fn expect(&self, e: &Expr, want: Ty, what: &str) -> Result<Ty, String> {
        let got = self.ty(e)?;
        if got != want {
            return Err(format!(
                "{what} needs {}, found {}.",
                want.name(),
                got.name()
            ));
        }
        Ok(want)
    }

    /// Comparing an enum field with a string literal: the literal must be one of its values.
    fn enum_literal(&self, field: &Expr, lit: &Expr) -> Result<(), String> {
        if let (Expr::Path(p), Expr::Str(s)) = (field, lit)
            && let (_, Some(values)) = self.path(p)?
            && !values.contains(s)
        {
            return Err(format!(
                "`{}` has no value '{s}'. Its values are: {}.",
                p.join("."),
                values.join(", ")
            ));
        }
        Ok(())
    }
}
