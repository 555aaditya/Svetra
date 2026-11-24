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
        let role = self.role_as(k.get("as").copied(), &path)?;
        let check = match k.get("check") {
            Some(v) => Some(self.string(v, &format!("{path}.check"))?),
            None => None,
        };
        let only = match k.get("only") {
            Some(v) => {
                let fields = self.name_list(v, &format!("{path}.only"));
                if fields.is_empty() {
                    self.report(
                        Code::Svt029,
                        &format!("{path}.only"),
                        v,
                        "`only` must list at least one field.".into(),
                        "Name the fields an update may change, such as only: [status].",
                    );
                    return None;
                }
                Some(fields)
            }
            None => None,
        };
        let restrictive = k
            .get("restrictive")
            .is_some_and(|v| self.boolean(v, &format!("{path}.restrictive")));
        Some(Policy {
            name: pname,
            pos: pos(name),
            on,
            allow,
            when,
            check,
            only,
            restrictive,
            role,
        })
    }

    /// The `as` key of a policy or hole: who it applies to. Authenticated when absent.
    fn role_as(&mut self, node: Option<&Node>, path: &str) -> Option<Role> {
        let Some(v) = node else {
            return Some(Role::Authenticated);
        };
        match self.string(v, &format!("{path}.as"))?.as_str() {
            "authenticated" => Some(Role::Authenticated),
            "anon" => Some(Role::Anon),
            "anyone" => Some(Role::Anyone),
            _ => {
                self.report(
                    Code::Svt029,
                    &format!("{path}.as"),
                    v,
                    "`as` must be `authenticated`, `anyone` or `anon`.".into(),
                    "Remove `as` to use authenticated.",
                );
                None
            }
        }
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

    /// The `events` of a triggered hole: a non-empty list of insert, update and delete.
    fn events(&mut self, node: &Node, path: &str) -> Option<Vec<Action>> {
        let names = self.name_list(node, path);
        let mut out = Vec::new();
        for n in &names {
            let a = match n.name.as_str() {
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
                        format!("`{other}` is not an event a trigger fires on."),
                        "Use insert, update or delete.",
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
                "`events` must list at least one event.".into(),
                "For example: events: [update].",
            );
            return None;
        }
        Some(out)
    }

    pub(super) fn hole(&mut self, name: &Node, body: &Node) -> Option<Hole> {
        let hname = self.string(name, "holes")?;
        let path = format!("holes.{hname}");
        let entries = self.map(body, &path)?;
        // The kind decides which keys are allowed, so find it first.
        let Some(kind_node) = entries
            .iter()
            .find(|(k, _)| k.as_str() == Some("kind"))
            .map(|(_, v)| v)
        else {
            let msg = format!("Missing required key `kind` in {}.", super::show(&path));
            self.report(Code::Svt028, &path, name, msg, "Add `kind`.");
            return None;
        };
        let kind_name = self.string(kind_node, &format!("{path}.kind"))?;
        let allowed = match kind_name.as_str() {
            "function" => super::FUNCTION_HOLE_KEYS,
            "schedule" => super::SCHEDULE_HOLE_KEYS,
            "trigger" => super::TRIGGER_HOLE_KEYS,
            other => {
                self.report(
                    Code::Svt023,
                    &format!("{path}.kind"),
                    kind_node,
                    format!("`{other}` is not a kind of hole."),
                    &format!("Use one of: {}.", super::HOLE_KINDS.join(", ")),
                );
                return None;
            }
        };
        // A key another kind takes gets its own message, which says why it doesn't apply here.
        let mut mine = Vec::new();
        for (key, value) in entries {
            match key.as_str() {
                Some(k) if super::HOLE_KEYS.contains(&k) && !allowed.contains(&k) => {
                    let msg = format!(
                        "`{k}` isn't allowed on a `{kind_name}` hole in {}: {}",
                        super::show(&path),
                        why_not(&kind_name, k)
                    );
                    let hint = format!(
                        "Remove it. A {kind_name} hole takes: {}.",
                        allowed.join(", ")
                    );
                    self.report(Code::Svt020, &super::join(&path, k), key, msg, &hint);
                }
                _ => mine.push((key.clone(), value.clone())),
            }
        }
        let k = self.keys(&mine, &path, allowed);
        let kind = match kind_name.as_str() {
            "schedule" => {
                let node = self.need(&k, "schedule", &path, name)?;
                let cron = self.string(node, &format!("{path}.schedule"))?;
                HoleKind::Schedule(Schedule {
                    cron,
                    pos: pos(node),
                })
            }
            "trigger" => {
                let on = self.need(&k, "on", &path, name);
                let events = self.need(&k, "events", &path, name);
                let (on, events) = (on?, events?);
                let on_ref = NameRef {
                    name: self.string(on, &format!("{path}.on"))?,
                    pos: pos(on),
                };
                HoleKind::Trigger(Trigger {
                    on: on_ref,
                    events: self.events(events, &format!("{path}.events"))?,
                    pos: pos(events),
                })
            }
            _ => HoleKind::Function,
        };
        let mut hole = Hole {
            name: hname,
            pos: pos(name),
            kind,
            input: Vec::new(),
            output: None,
            reads: Vec::new(),
            writes: Vec::new(),
            grants: Vec::new(),
            role: Role::Authenticated,
            when: None,
            audit: false,
        };
        hole.role = self.role_as(k.get("as").copied(), &path)?;
        if let Some(v) = k.get("when") {
            hole.when = self.string(v, &format!("{path}.when"));
        }
        if let Some(v) = k.get("audit") {
            hole.audit = self.boolean(v, &format!("{path}.audit"));
        }
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
        for (key, out) in [
            ("reads", &mut hole.reads),
            ("writes", &mut hole.writes),
            ("grants", &mut hole.grants),
        ] {
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

/// Why a key of another kind of hole doesn't apply to `kind`.
fn why_not(kind: &str, key: &str) -> &'static str {
    match (kind, key) {
        ("schedule", "input" | "output") => {
            "nobody passes or receives it, because the clock runs it."
        }
        ("trigger", "input" | "output") => {
            "the changed row is the input: the body gets it as `p_old` and `p_new`."
        }
        ("schedule", "as") => "nobody calls it, because the clock runs it.",
        ("trigger", "as") => "nobody calls it, because a change to a row runs it.",
        ("schedule", "when") => {
            "there is no call to guard. Write the check in the body, such as `if not exists (...) then return; end if;`."
        }
        ("schedule", "audit") => "an audited hole records a call by a person, and nobody calls it.",
        ("trigger", "audit") => {
            "an audited hole records a call by a person, and nobody calls it. Changes to an audited entity are logged anyway."
        }
        ("function" | "trigger", "schedule") => "only `kind: schedule` runs on a clock.",
        ("function" | "schedule", "on" | "events") => {
            "only `kind: trigger` runs when a row changes."
        }
        _ => "this kind doesn't take it.",
    }
}
