//! `svetra graph`: a Mermaid flowchart of entities, links, access and custom code
//! (docs/understand.md). Node ids are prefixed so names like `end` can't clash with Mermaid.

use super::context::type_name;
use crate::model::*;

pub fn graph(doc: &Document) -> String {
    let mut out = String::from("flowchart LR\n");
    let has_user = doc.policies.iter().any(|p| p.role == Role::Authenticated)
        || doc
            .entities
            .iter()
            .flat_map(|e| &e.fields)
            .any(|f| f.ty == FieldType::User);
    let has_anon = doc.policies.iter().any(|p| p.role == Role::Anon);
    let has_anyone = doc.policies.iter().any(|p| p.role == Role::Anyone);
    if has_user {
        out += "  person((\"👤 Signed-in person\"))\n";
    }
    if has_anon {
        out += "  visitor((\"🚪 Not signed in\"))\n";
    }
    if has_anyone {
        out += "  anyone((\"🌐 Anyone\"))\n";
    }
    for e in &doc.entities {
        let fields: Vec<String> = e
            .fields
            .iter()
            .filter(|f| !f.primary)
            .map(|f| {
                format!(
                    "{}: {}{}",
                    f.name,
                    short(&f.ty),
                    if f.required { "!" } else { "" }
                )
            })
            .collect();
        let lock = if e.private { "🔒 " } else { "" };
        out += &format!(
            "  e_{}[\"{lock}{}<br/>{}\"]\n",
            e.name,
            e.name,
            fields.join("<br/>")
        );
    }
    for h in &doc.holes {
        out += &format!("  h_{}([\"⚙️ {}\"])\n", h.name, h.name);
    }
    for e in &doc.entities {
        for f in &e.fields {
            match &f.ty {
                FieldType::Ref(r) => {
                    out += &format!("  e_{} -- {} --> e_{}\n", e.name, f.name, r.name)
                }
                FieldType::User => out += &format!("  e_{} -- {} --> person\n", e.name, f.name),
                _ => {}
            }
        }
    }
    for (role, node) in [
        (Role::Authenticated, "person"),
        (Role::Anon, "visitor"),
        (Role::Anyone, "anyone"),
    ] {
        for e in &doc.entities {
            let mut verbs: Vec<&str> = Vec::new();
            for p in doc
                .policies
                .iter()
                .filter(|p| p.role == role && p.on.name == e.name)
            {
                for a in &p.allow {
                    let v = match a {
                        Action::Select => "sees",
                        Action::Insert => "creates",
                        Action::Update => "changes",
                        Action::Delete => "deletes",
                    };
                    if !verbs.contains(&v) {
                        verbs.push(v);
                    }
                }
            }
            if !verbs.is_empty() {
                out += &format!("  {node} -. \"{}\" .-> e_{}\n", verbs.join(", "), e.name);
            }
        }
    }
    for h in &doc.holes {
        for r in &h.reads {
            out += &format!("  h_{} -. reads .-> e_{}\n", h.name, r.name);
        }
        for w in &h.writes {
            out += &format!("  h_{} == changes ==> e_{}\n", h.name, w.name);
        }
    }
    out
}

/// Short type names for diagram labels: parameters are left out to keep nodes small.
fn short(t: &FieldType) -> String {
    match t {
        FieldType::Enum(_) => "enum".into(),
        FieldType::Decimal { .. } => "decimal".into(),
        FieldType::Money { .. } => "money".into(),
        FieldType::Ref(r) => format!("→ {}", r.name),
        other => type_name(other),
    }
}
