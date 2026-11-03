//! Policies and holes (SVT023, SVT029).

use super::{Cx, pos};
use crate::diag::Code;
use crate::model::*;
use crate::yaml::{Node, Value};
use std::collections::HashMap;

const BARE_TYPES: &[&str] = &[
    "text",
    "int",
    "float",
    "bool",
    "uuid",
    "timestamp",
    "date",
    "json",
];

impl Cx {
    pub(super) fn policy(&mut self, name: &Node, body: &Node) -> Option<Policy> {
        let pname = self.string(name, "policies")?;
        let path = format!("policies.{pname}");
        let entries = self.map(body, &path)?;
        let k = self.keys(entries, &path, super::POLICY_KEYS);
        let on = self.need(&k, "on", &path, name);
        let allow = self.need(&k, "allow", &path, name);
        let when = self.need(&k, "when", &path, name);
        let on = NameRef {
            name: self.string(on?, &format!("{path}.on"))?,
            pos: pos(on?),
        };
        let allow = self.actions(allow?, &format!("{path}.allow"))?;
        let when = self.string(when?, &format!("{path}.when"))?;
        let role = match k.get("as") {
            None => Role::Authenticated,
            Some(v) => match self.string(v, &format!("{path}.as"))?.as_str() {
                "authenticated" => Role::Authenticated,
                "anon" => Role::Anon,
                _ => {
                    self.report(
                        Code::Svt029,
                        &format!("{path}.as"),
                        v,
                        "`as` must be `authenticated` or `anon`.".into(),
                        "Remove `as` to use authenticated.",
                    );
                    return None;
                }
            },
        };
        Some(Policy {
            name: pname,
            pos: pos(name),
            on,
            allow,
            when,
            role,
        })
    }

    fn actions(&mut self, node: &Node, path: &str) -> Option<Vec<Action>> {
        let names = self.name_list(node, path);
        let mut out = Vec::new();
        for n in &names {
            let a = match n.name.as_str() {
                "select" => Action::Select,
                "insert" => Action::Insert,
                "update" => Action::Update,
                "delete" => Action::Delete,
                other => {
                    let at = Node {
                        value: Value::Null,
                        line: n.pos.line,
                        col: n.pos.col,
                    };
                    self.report(
                        Code::Svt029,
                        path,
                        &at,
                        format!("Unknown action `{other}`."),
                        "Use select, insert, update or delete.",
                    );
                    continue;
                }
            };
            if !out.contains(&a) {
                out.push(a);
            }
        }
        if out.is_empty() && names.is_empty() {
            self.report(
                Code::Svt029,
                path,
                node,
                "`allow` must list at least one action.".into(),
                "For example: allow: [select].",
            );
            return None;
        }
        Some(out)
    }

    pub(super) fn hole(&mut self, name: &Node, body: &Node) -> Option<Hole> {
        let hname = self.string(name, "holes")?;
        let path = format!("holes.{hname}");
        let entries = self.map(body, &path)?;
        let k = self.keys(entries, &path, super::HOLE_KEYS);
        let kind = self.need(&k, "kind", &path, name)?;
        if self.string(kind, &format!("{path}.kind"))? != "function" {
            self.report(
                Code::Svt023,
                &format!("{path}.kind"),
                kind,
                "Version 0.1 supports only `kind: function`.".into(),
                "Write kind: function.",
            );
            return None;
        }
        let mut hole = Hole {
            name: hname,
            pos: pos(name),
            input: Vec::new(),
            output: None,
            reads: Vec::new(),
            writes: Vec::new(),
        };
        if let Some(v) = k.get("input") {
            let ipath = format!("{path}.input");
            for (pn, pt) in self.map(v, &ipath).unwrap_or(&[]) {
                let Some(pname) = self.string(pn, &ipath) else {
                    continue;
                };
                let ppath = format!("{ipath}.{pname}");
                if let Some(ty) = self.param_type(pt, &ppath, false) {
                    hole.input.push(Param {
                        name: pname,
                        pos: pos(pn),
                        ty,
                    });
                }
            }
        }
        if let Some(v) = k.get("output") {
            hole.output = self.param_type(v, &format!("{path}.output"), true);
        }
        for (key, out) in [("reads", &mut hole.reads), ("writes", &mut hole.writes)] {
            if let Some(v) = k.get(key) {
                *out = self.name_list(v, &format!("{path}.{key}"));
            }
        }
        Some(hole)
    }

    /// `Entity.field`, a bare type, a full field spec, or (outputs only) an entity name.
    fn param_type(&mut self, node: &Node, path: &str, allow_entity: bool) -> Option<ParamType> {
        match &node.value {
            Value::Str(s) => {
                if let Some((entity, field)) = s.split_once('.') {
                    let entity = NameRef {
                        name: entity.to_string(),
                        pos: pos(node),
                    };
                    return Some(ParamType::FieldOf {
                        entity,
                        field: field.to_string(),
                    });
                }
                if allow_entity && s.starts_with(|c: char| c.is_ascii_uppercase()) {
                    return Some(ParamType::Entity(NameRef {
                        name: s.clone(),
                        pos: pos(node),
                    }));
                }
                self.bare_type(s, path, node).map(ParamType::Type)
            }
            Value::Map(entries) => {
                let k = self.keys(entries, path, super::PARAM_SPEC_KEYS);
                self.field_type(&k, path, node).map(ParamType::Type)
            }
            _ => {
                let msg = format!("Expected a type at `{path}`, found a {}.", node.kind());
                self.report(
                    Code::Svt029,
                    path,
                    node,
                    msg,
                    "Use Entity.field, a type name, or a full field spec.",
                );
                None
            }
        }
    }

    /// A hole parameter written as a bare type name. Only types without options qualify.
    pub(super) fn bare_type(&mut self, name: &str, path: &str, at: &Node) -> Option<FieldType> {
        if !BARE_TYPES.contains(&name) {
            let msg = format!("`{name}` is not a bare type name.");
            let hint = "Use Entity.field, one of text, int, float, bool, uuid, timestamp, date, json, or a full spec like { type: money, precision: 18, scale: 6 }.";
            self.report(Code::Svt023, path, at, msg, hint);
            return None;
        }
        let mut k = HashMap::new();
        let n = Node {
            value: Value::Str(name.into()),
            line: at.line,
            col: at.col,
        };
        k.insert("type", &n);
        self.field_type(&k, path, at)
    }
}
