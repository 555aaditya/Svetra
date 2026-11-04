//! Load a `svetra.yaml` source into the typed model, collecting structural diagnostics
//! (SVT019, SVT020, SVT023, SVT025, SVT028, SVT029) instead of stopping at the first one.

use crate::diag::{Code, Diagnostic};
use crate::model::*;
use crate::yaml::{self, Node, Value};
use std::collections::HashMap;

mod access;
mod field;

pub const FORMAT_VERSION: &str = "0.1";

// The keys each part of the format accepts. The JSON Schema is tested against these.
pub const TOP_KEYS: &[&str] = &["svetra", "system", "entities", "policies", "holes", "roles"];
pub const ROLE_KEYS: &[&str] = &["description", "scope"];
pub const ENTITY_KEYS: &[&str] = &["fields", "constraints", "unique", "indexes", "private"];
pub const FIELD_KEYS: &[&str] = &[
    "type",
    "required",
    "primary",
    "unique",
    "default",
    "auto",
    "values",
    "precision",
    "scale",
    "entity",
];
pub const POLICY_KEYS: &[&str] = &["on", "allow", "when", "as"];
pub const HOLE_KEYS: &[&str] = &["kind", "input", "output", "reads", "writes"];
pub const TYPE_NAMES: &[&str] = &[
    "text",
    "int",
    "float",
    "bool",
    "uuid",
    "timestamp",
    "date",
    "decimal",
    "money",
    "enum",
    "json",
    "ref",
    "user",
];
pub const PARAM_SPEC_KEYS: &[&str] = &["type", "values", "precision", "scale", "entity"];

type Entries = [(Node, Node)];

/// Load `src`. The document is best effort: parts with errors are skipped or defaulted,
/// and every problem is in the returned diagnostics.
pub fn load(src: &str) -> (Document, Vec<Diagnostic>) {
    let mut cx = Cx::default();
    let doc = match yaml::parse(src) {
        Ok(root) => cx.document(&root),
        Err(e) => {
            cx.diags.push(Diagnostic {
                code: Code::Svt019,
                path: String::new(),
                line: e.line,
                col: e.col,
                message: format!("Not valid YAML: {}.", e.message),
                hint: "Fix the YAML syntax at this position.".into(),
            });
            empty_document()
        }
    };
    (doc, cx.diags)
}

fn empty_document() -> Document {
    Document {
        version: String::new(),
        system: NameRef {
            name: String::new(),
            pos: Pos::default(),
        },
        entities: Vec::new(),
        policies: Vec::new(),
        holes: Vec::new(),
        roles: Vec::new(),
    }
}

pub(crate) fn pos(n: &Node) -> Pos {
    Pos {
        line: n.line,
        col: n.col,
    }
}

#[derive(Default)]
pub(crate) struct Cx {
    pub diags: Vec<Diagnostic>,
}

impl Cx {
    pub fn report(&mut self, code: Code, path: &str, at: &Node, message: String, hint: &str) {
        self.diags.push(Diagnostic {
            code,
            path: path.to_string(),
            line: at.line,
            col: at.col,
            message,
            hint: hint.to_string(),
        });
    }

    /// Expect a map at `node`, or report SVT029.
    pub fn map<'a>(&mut self, node: &'a Node, path: &str) -> Option<&'a Entries> {
        match &node.value {
            Value::Map(entries) => Some(entries),
            _ => {
                let msg = format!("Expected a map at {}, found a {}.", show(path), node.kind());
                self.report(
                    Code::Svt029,
                    path,
                    node,
                    msg,
                    "Write this value as a YAML map.",
                );
                None
            }
        }
    }

    /// Index a map by key, reporting keys not in `allowed` (SVT020) and non-string keys (SVT029).
    pub fn keys<'a>(
        &mut self,
        entries: &'a Entries,
        path: &str,
        allowed: &[&str],
    ) -> HashMap<&'a str, &'a Node> {
        let mut out = HashMap::new();
        for (k, v) in entries {
            match k.as_str() {
                Some(key) if allowed.contains(&key) => {
                    out.insert(key, v);
                }
                Some(key) => {
                    let msg = format!("Unknown key `{key}` in {}.", show(path));
                    let hint = format!("Remove it. Allowed keys: {}.", allowed.join(", "));
                    self.report(Code::Svt020, &join(path, key), k, msg, &hint);
                }
                None => {
                    let msg = format!(
                        "Keys in {} must be strings, found a {}.",
                        show(path),
                        k.kind()
                    );
                    self.report(Code::Svt029, path, k, msg, "Use a plain name as the key.");
                }
            }
        }
        out
    }

    /// Report SVT028 when `key` is missing from a map that lives at `at`.
    pub fn need<'a>(
        &mut self,
        keys: &HashMap<&str, &'a Node>,
        key: &str,
        path: &str,
        at: &Node,
    ) -> Option<&'a Node> {
        let found = keys.get(key).copied();
        if found.is_none() {
            let msg = format!("Missing required key `{key}` in {}.", show(path));
            self.report(Code::Svt028, path, at, msg, &format!("Add `{key}`."));
        }
        found
    }

    pub fn string(&mut self, node: &Node, path: &str) -> Option<String> {
        match &node.value {
            Value::Str(s) => Some(s.clone()),
            _ => {
                let msg = format!(
                    "Expected a string at {}, found a {}.",
                    show(path),
                    node.kind()
                );
                self.report(
                    Code::Svt029,
                    path,
                    node,
                    msg,
                    "Write this value as a string.",
                );
                None
            }
        }
    }

    pub fn boolean(&mut self, node: &Node, path: &str) -> bool {
        match node.value {
            Value::Bool(b) => b,
            _ => {
                let msg = format!(
                    "Expected true or false at {}, found a {}.",
                    show(path),
                    node.kind()
                );
                self.report(Code::Svt029, path, node, msg, "Write true or false.");
                false
            }
        }
    }

    pub fn name_list(&mut self, node: &Node, path: &str) -> Vec<NameRef> {
        let Value::Seq(items) = &node.value else {
            let msg = format!(
                "Expected a list at {}, found a {}.",
                show(path),
                node.kind()
            );
            self.report(
                Code::Svt029,
                path,
                node,
                msg,
                "Write this value as a YAML list, like [a, b].",
            );
            return Vec::new();
        };
        items
            .iter()
            .filter_map(|n| {
                self.string(n, path)
                    .map(|name| NameRef { name, pos: pos(n) })
            })
            .collect()
    }

    fn document(&mut self, root: &Node) -> Document {
        let mut doc = empty_document();
        let Some(entries) = self.map(root, "") else {
            return doc;
        };
        let k = self.keys(entries, "", TOP_KEYS);
        if let Some(v) = self.need(&k, "svetra", "", root) {
            self.version(v, &mut doc);
        }
        if let Some(v) = self.need(&k, "system", "", root)
            && let Some(name) = self.string(v, "system")
        {
            doc.system = NameRef { name, pos: pos(v) };
        }
        if let Some(v) = self.need(&k, "entities", "", root)
            && let Some(es) = self.map(v, "entities")
        {
            doc.entities = es.iter().filter_map(|(n, e)| self.entity(n, e)).collect();
        }
        if let Some(v) = k.get("policies")
            && let Some(ps) = self.map(v, "policies")
        {
            doc.policies = ps.iter().filter_map(|(n, p)| self.policy(n, p)).collect();
        }
        if let Some(v) = k.get("holes")
            && let Some(hs) = self.map(v, "holes")
        {
            doc.holes = hs.iter().filter_map(|(n, h)| self.hole(n, h)).collect();
        }
        if let Some(v) = k.get("roles")
            && let Some(rs) = self.map(v, "roles")
        {
            doc.roles = rs.iter().filter_map(|(n, r)| self.role(n, r)).collect();
        }
        doc
    }

    fn role(&mut self, name: &Node, body: &Node) -> Option<RoleDef> {
        let rname = self.string(name, "roles")?;
        let path = format!("roles.{rname}");
        let mut role = RoleDef {
            name: rname,
            pos: pos(name),
            description: None,
            scope: None,
        };
        if matches!(body.value, Value::Null) {
            return Some(role);
        }
        let entries = self.map(body, &path)?;
        let k = self.keys(entries, &path, ROLE_KEYS);
        if let Some(v) = k.get("description") {
            role.description = self.string(v, &format!("{path}.description"));
        }
        if let Some(v) = k.get("scope") {
            role.scope = self
                .string(v, &format!("{path}.scope"))
                .map(|name| NameRef { name, pos: pos(v) });
        }
        Some(role)
    }

    fn version(&mut self, v: &Node, doc: &mut Document) {
        match &v.value {
            Value::Str(s) if s == FORMAT_VERSION => doc.version = s.clone(),
            Value::Float(_) | Value::Int(_) => {
                let msg = "The format version must be a quoted string.".to_string();
                self.report(
                    Code::Svt025,
                    "svetra",
                    v,
                    msg,
                    "Write svetra: \"0.1\" with quotes.",
                );
            }
            _ => {
                let msg =
                    format!("Unsupported format version; this Svetra reads \"{FORMAT_VERSION}\".");
                self.report(Code::Svt025, "svetra", v, msg, "Set svetra: \"0.1\".");
            }
        }
    }

    fn entity(&mut self, name: &Node, body: &Node) -> Option<Entity> {
        let ename = self.string(name, "entities")?;
        let path = format!("entities.{ename}");
        let entries = self.map(body, &path)?;
        let k = self.keys(entries, &path, ENTITY_KEYS);
        let mut entity = Entity {
            name: ename,
            pos: pos(name),
            fields: Vec::new(),
            constraints: Vec::new(),
            unique: Vec::new(),
            indexes: Vec::new(),
            private: false,
        };
        if let Some(v) = self.need(&k, "fields", &path, name) {
            let fpath = format!("{path}.fields");
            if let Some(fs) = self.map(v, &fpath) {
                entity.fields = fs
                    .iter()
                    .filter_map(|(n, f)| self.field(n, f, &fpath))
                    .collect();
            }
        }
        if let Some(v) = k.get("constraints") {
            let cpath = format!("{path}.constraints");
            if let Some(cs) = self.map(v, &cpath) {
                for (n, e) in cs {
                    let (Some(cname), Some(expr)) =
                        (self.string(n, &cpath), self.string(e, &cpath))
                    else {
                        continue;
                    };
                    entity.constraints.push(Constraint {
                        name: cname,
                        pos: pos(n),
                        expr,
                    });
                }
            }
        }
        for (key, out) in [
            ("unique", &mut entity.unique),
            ("indexes", &mut entity.indexes),
        ] {
            if let Some(v) = k.get(key) {
                *out = self.field_lists(v, &format!("{path}.{key}"));
            }
        }
        if let Some(v) = k.get("private") {
            entity.private = self.boolean(v, &format!("{path}.private"));
        }
        Some(entity)
    }

    fn field_lists(&mut self, node: &Node, path: &str) -> Vec<FieldList> {
        let Value::Seq(items) = &node.value else {
            let msg = format!(
                "Expected a list of field lists at {}, found a {}.",
                show(path),
                node.kind()
            );
            self.report(
                Code::Svt029,
                path,
                node,
                msg,
                "Write it like [[a, b], [c]].",
            );
            return Vec::new();
        };
        items
            .iter()
            .map(|n| FieldList {
                fields: self.name_list(n, path),
                pos: pos(n),
            })
            .collect()
    }
}

pub(crate) fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

pub(crate) fn show(path: &str) -> String {
    if path.is_empty() {
        "the top level".into()
    } else {
        format!("`{path}`")
    }
}
