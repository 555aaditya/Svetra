//! `svetra context`: the whole graph in as few tokens as possible, for agents
//! (docs/understand.md). Uses the format's own type names, so agents can patch from it.

use crate::model::*;

pub fn context(doc: &Document) -> String {
    let mut out = format!("{} (svetra {})\n", doc.system.name, doc.version);
    if !doc.roles.is_empty() {
        let roles: Vec<String> = doc
            .roles
            .iter()
            .map(|r| {
                r.scope
                    .as_ref()
                    .map_or(r.name.clone(), |s| format!("{}({})", r.name, s.name))
            })
            .collect();
        out += &format!("roles {}\n", roles.join("; "));
    }
    for e in &doc.entities {
        let fields: Vec<String> = e.fields.iter().map(field).collect();
        let mut line = format!(
            "{}{}: {}",
            if e.private { "private " } else { "" },
            e.name,
            fields.join("; ")
        );
        if !e.constraints.is_empty() {
            let checks: Vec<String> = e
                .constraints
                .iter()
                .map(|c| format!("{}: {}", c.name, c.expr))
                .collect();
            line += &format!(" | check {}", checks.join("; "));
        }
        let lists = |ls: &[FieldList]| {
            ls.iter()
                .map(|l| {
                    format!(
                        "({})",
                        l.fields
                            .iter()
                            .map(|x| x.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        if !e.unique.is_empty() {
            line += &format!(" | unique {}", lists(&e.unique));
        }
        if !e.indexes.is_empty() {
            line += &format!(" | index {}", lists(&e.indexes));
        }
        out += &line;
        out.push('\n');
    }
    for p in &doc.policies {
        let actions: Vec<&str> = p
            .allow
            .iter()
            .map(|a| match a {
                Action::Select => "select",
                Action::Insert => "insert",
                Action::Update => "update",
                Action::Delete => "delete",
            })
            .collect();
        let role = match p.role {
            Role::Authenticated => "authenticated",
            Role::Anon => "anon",
            Role::Anyone => "anyone",
        };
        out += &format!(
            "policy {}: {} {} {role} when {}\n",
            p.name,
            p.on.name,
            actions.join(","),
            p.when
        );
    }
    for h in &doc.holes {
        let params: Vec<String> = h
            .input
            .iter()
            .map(|p| format!("{} {}", p.name, param(&p.ty)))
            .collect();
        let mut line = format!(
            "hole {}: ({}) -> {}",
            h.name,
            params.join(", "),
            h.output.as_ref().map_or("none".into(), param)
        );
        let names = |rs: &[NameRef]| {
            rs.iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        if !h.reads.is_empty() {
            line += &format!("; reads {}", names(&h.reads));
        }
        if !h.writes.is_empty() {
            line += &format!("; writes {}", names(&h.writes));
        }
        out += &line;
        out.push('\n');
    }
    out
}

pub fn type_name(t: &FieldType) -> String {
    match t {
        FieldType::Text => "text".into(),
        FieldType::Int => "int".into(),
        FieldType::Float => "float".into(),
        FieldType::Bool => "bool".into(),
        FieldType::Uuid => "uuid".into(),
        FieldType::Timestamp => "timestamp".into(),
        FieldType::Date => "date".into(),
        FieldType::Decimal { precision, scale } => format!("decimal({precision},{scale})"),
        FieldType::Money { precision, scale } => format!("money({precision},{scale})"),
        FieldType::Enum(values) => format!("enum({})", values.join("|")),
        FieldType::Json => "json".into(),
        FieldType::Ref(r) => format!("ref({})", r.name),
        FieldType::User => "user".into(),
    }
}

fn field(f: &Field) -> String {
    let mut s = format!("{} {}", f.name, type_name(&f.ty));
    if f.primary {
        s += " pk";
    } else if f.required {
        s.push('!');
    }
    if f.unique && !f.primary {
        s += " unique";
    }
    if let Some(d) = &f.default {
        s += &match d {
            Literal::Bool(b) => format!("={b}"),
            Literal::Int(i) => format!("={i}"),
            Literal::Float(x) => format!("={x}"),
            Literal::Str(v) => format!("={v}"),
        };
    } else if f.auto_now {
        s += "=now";
    }
    s
}

fn param(t: &ParamType) -> String {
    match t {
        ParamType::FieldOf { entity, field } => format!("{}.{field}", entity.name),
        ParamType::Entity(r) => r.name.clone(),
        ParamType::Type(t) => type_name(t),
    }
}
