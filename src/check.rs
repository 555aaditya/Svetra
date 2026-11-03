//! Rules that need the whole document: names, duplicates, primary keys, references,
//! access, money, indexes and expressions.

use crate::diag::{Code, Diagnostic};
use crate::model::*;
use crate::typeck;
use std::collections::{HashMap, HashSet};

/// Words that mark a field as money for SVT005.
const MONEY_WORDS: &[&str] = &["amount", "price", "balance", "cost", "total", "fee"];

pub fn check(doc: &Document) -> Vec<Diagnostic> {
    let mut ck = Checker {
        entities: doc.entities.iter().map(|e| (e.name.as_str(), e)).collect(),
        out: Vec::new(),
    };
    ck.names(doc);
    ck.duplicates(doc);
    for e in &doc.entities {
        ck.entity(e);
    }
    ck.policies(doc);
    ck.holes(doc);
    ck.access(doc);
    ck.expressions(doc);
    ck.out
}

struct Checker<'a> {
    entities: HashMap<&'a str, &'a Entity>,
    out: Vec<Diagnostic>,
}

fn pascal(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn snake(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl<'a> Checker<'a> {
    fn d(&mut self, code: Code, path: String, at: Pos, message: String, hint: &str) {
        self.out.push(Diagnostic {
            code,
            path,
            line: at.line,
            col: at.col,
            message,
            hint: hint.into(),
        });
    }

    fn name(&mut self, kind: &str, name: &str, path: String, at: Pos, pascal_case: bool) {
        let (ok, style) = if pascal_case {
            (pascal(name), "PascalCase")
        } else {
            (snake(name), "snake_case")
        };
        if !ok {
            self.d(
                Code::Svt021,
                path,
                at,
                format!("{kind} name `{name}` must be {style}."),
                &format!("Rename it in {style}."),
            );
        }
    }

    fn names(&mut self, doc: &Document) {
        if !doc.system.name.is_empty() {
            self.name(
                "System",
                &doc.system.name,
                "system".into(),
                doc.system.pos,
                true,
            );
        }
        for e in &doc.entities {
            let ep = format!("entities.{}", e.name);
            self.name("Entity", &e.name, ep.clone(), e.pos, true);
            for f in &e.fields {
                self.name(
                    "Field",
                    &f.name,
                    format!("{ep}.fields.{}", f.name),
                    f.pos,
                    false,
                );
            }
            for c in &e.constraints {
                self.name(
                    "Constraint",
                    &c.name,
                    format!("{ep}.constraints.{}", c.name),
                    c.pos,
                    false,
                );
            }
        }
        for p in &doc.policies {
            self.name(
                "Policy",
                &p.name,
                format!("policies.{}", p.name),
                p.pos,
                false,
            );
        }
        for h in &doc.holes {
            self.name("Hole", &h.name, format!("holes.{}", h.name), h.pos, false);
        }
    }

    /// Constraint, policy and hole names become database object names, so they must be unique file-wide.
    fn duplicates(&mut self, doc: &Document) {
        let mut seen = HashSet::new();
        let constraints = doc.entities.iter().flat_map(|e| {
            e.constraints.iter().map(move |c| {
                (
                    c.name.as_str(),
                    format!("entities.{}.constraints.{}", e.name, c.name),
                    c.pos,
                )
            })
        });
        let policies = doc
            .policies
            .iter()
            .map(|p| (p.name.as_str(), format!("policies.{}", p.name), p.pos));
        let holes = doc
            .holes
            .iter()
            .map(|h| (h.name.as_str(), format!("holes.{}", h.name), h.pos));
        for (name, path, at) in constraints.chain(policies).chain(holes) {
            if !seen.insert(name) {
                let hint = "Constraint, policy and hole names must be unique across the file.";
                self.d(
                    Code::Svt024,
                    path,
                    at,
                    format!("The name `{name}` is used more than once."),
                    hint,
                );
            }
        }
    }

    fn entity(&mut self, e: &Entity) {
        let ep = format!("entities.{}", e.name);
        let primaries = e.fields.iter().filter(|f| f.primary).count();
        if primaries != 1 {
            let msg = format!(
                "{} has {primaries} primary fields; it needs exactly one.",
                e.name
            );
            self.d(
                Code::Svt022,
                ep.clone(),
                e.pos,
                msg,
                "Mark one field primary: true, usually id: { type: uuid, primary: true }.",
            );
        }
        let names: HashSet<&str> = e.fields.iter().map(|f| f.name.as_str()).collect();
        for (key, lists) in [("unique", &e.unique), ("indexes", &e.indexes)] {
            for f in lists.iter().flat_map(|l| &l.fields) {
                if !names.contains(f.name.as_str()) {
                    let msg = format!("`{key}` names `{}`, which {} doesn't have.", f.name, e.name);
                    self.d(
                        Code::Svt026,
                        format!("{ep}.{key}"),
                        f.pos,
                        msg,
                        "Use a field of this entity.",
                    );
                }
            }
        }
        for f in &e.fields {
            let fp = format!("{ep}.fields.{}", f.name);
            if let FieldType::Ref(target) = &f.ty {
                self.reference(target, &fp);
            }
            // `user` fields reference auth.users and are what policies filter on, so they need indexes too.
            if matches!(f.ty, FieldType::Ref(_) | FieldType::User) {
                let leads = |l: &FieldList| l.fields.first().is_some_and(|x| x.name == f.name);
                if !f.unique && !e.unique.iter().chain(&e.indexes).any(leads) {
                    let msg = format!(
                        "`{}` is a reference with no index leading on it; the compiler will add one.",
                        f.name
                    );
                    self.d(
                        Code::Svt010,
                        fp.clone(),
                        f.pos,
                        msg,
                        "Nothing to do, or declare the index yourself.",
                    );
                }
            }
            let words: Vec<&str> = f.name.split('_').collect();
            if f.ty == FieldType::Float && words.iter().any(|w| MONEY_WORDS.contains(w)) {
                let msg = format!(
                    "`{}` looks like money but uses float, which loses cents.",
                    f.name
                );
                self.d(
                    Code::Svt005,
                    fp,
                    f.pos,
                    msg,
                    "Use { type: money, precision: 18, scale: 6 }.",
                );
            }
        }
    }

    /// SVT002 for a reference to an entity. Returns the entity if it exists.
    fn reference(&mut self, r: &NameRef, path: &str) -> Option<&'a Entity> {
        let found = self.entities.get(r.name.as_str()).copied();
        if found.is_none() {
            let msg = format!("No entity named `{}`.", r.name);
            self.d(
                Code::Svt002,
                path.into(),
                r.pos,
                msg,
                "Use an entity declared under `entities`.",
            );
        }
        found
    }

    fn policies(&mut self, doc: &Document) {
        for p in &doc.policies {
            let pp = format!("policies.{}.on", p.name);
            if let Some(e) = self.reference(&p.on, &pp)
                && e.private
            {
                let msg = format!(
                    "{} is private, so policy `{}` has no effect.",
                    e.name, p.name
                );
                self.d(
                    Code::Svt027,
                    pp,
                    p.pos,
                    msg,
                    "Remove the policy, or make the entity public.",
                );
            }
        }
    }

    fn holes(&mut self, doc: &Document) {
        for h in &doc.holes {
            let hp = format!("holes.{}", h.name);
            for r in h.reads.iter().chain(&h.writes) {
                self.reference(r, &hp);
            }
            let params = h
                .input
                .iter()
                .map(|p| (&p.ty, format!("{hp}.input.{}", p.name)));
            let output = h.output.iter().map(|t| (t, format!("{hp}.output")));
            for (ty, path) in params.chain(output) {
                match ty {
                    ParamType::Entity(r) => {
                        self.reference(r, &path);
                    }
                    ParamType::FieldOf { entity, field } => {
                        if let Some(e) = self.reference(entity, &path)
                            && !e.fields.iter().any(|f| &f.name == field)
                        {
                            let msg = format!("{} has no field `{field}`.", e.name);
                            self.d(
                                Code::Svt002,
                                path,
                                entity.pos,
                                msg,
                                "Use a field of that entity.",
                            );
                        }
                    }
                    ParamType::Type(FieldType::Ref(r)) => {
                        self.reference(r, &path);
                    }
                    ParamType::Type(_) => {}
                }
            }
        }
    }

    /// SVT003: constraints and policy conditions must type-check as booleans in their scope.
    fn expressions(&mut self, doc: &Document) {
        let entities = self.entities.clone();
        for e in &doc.entities {
            let scope = typeck::Scope {
                entity: e,
                entities: &entities,
                policy: false,
            };
            for c in &e.constraints {
                if let Err(msg) = typeck::check(&c.expr, &scope) {
                    self.d(
                        Code::Svt003,
                        format!("entities.{}.constraints.{}", e.name, c.name),
                        c.pos,
                        msg,
                        "Fix the expression; see Expressions in docs/format.md.",
                    );
                }
            }
        }
        for p in &doc.policies {
            let Some(e) = entities.get(p.on.name.as_str()) else {
                continue;
            };
            let scope = typeck::Scope {
                entity: e,
                entities: &entities,
                policy: true,
            };
            if let Err(msg) = typeck::check(&p.when, &scope) {
                self.d(
                    Code::Svt003,
                    format!("policies.{}.when", p.name),
                    p.pos,
                    msg,
                    "Fix the expression; see Expressions in docs/format.md.",
                );
            }
        }
    }

    /// SVT001: exposed entities need a policy. SVT004: every entity needs some write path.
    fn access(&mut self, doc: &Document) {
        for e in &doc.entities {
            let policies: Vec<&Policy> = doc
                .policies
                .iter()
                .filter(|p| p.on.name == e.name)
                .collect();
            let ep = format!("entities.{}", e.name);
            if !e.private && policies.is_empty() {
                let msg = format!("{} is exposed through the API but has no policy.", e.name);
                self.d(
                    Code::Svt001,
                    ep.clone(),
                    e.pos,
                    msg,
                    &format!("Add a policy on {}, or mark it private: true.", e.name),
                );
            }
            let api_write = policies.iter().any(|p| {
                p.allow
                    .iter()
                    .any(|a| matches!(a, Action::Insert | Action::Update))
            });
            let hole_write = doc
                .holes
                .iter()
                .any(|h| h.writes.iter().any(|w| w.name == e.name));
            if !api_write && !hole_write {
                let msg = format!(
                    "Nothing can write {}: no policy allows insert or update, and no hole writes it.",
                    e.name
                );
                self.d(
                    Code::Svt004,
                    ep,
                    e.pos,
                    msg,
                    "Add a write policy, or list it in a hole's `writes`.",
                );
            }
        }
    }
}
