//! The SQL schema (docs/compiler.md, SQL). Sections are emitted in a fixed order, and
//! entities and fields in file order, so the output is byte-identical for the same input.

use super::expr::{Ctx, sql as expr_sql};
use super::names::{lit, q, snake, table};
use crate::expr;
use crate::model::*;
use std::collections::HashMap;

/// Tables, foreign keys and indexes for a validated document.
pub fn schema(doc: &Document) -> String {
    let entities: HashMap<&str, &Entity> =
        doc.entities.iter().map(|e| (e.name.as_str(), e)).collect();
    let tables: Vec<String> = doc
        .entities
        .iter()
        .map(|e| create_table(e, &entities))
        .collect();
    let mut out = format!("-- Tables\n{}", tables.join("\n"));
    let fks: Vec<String> = doc
        .entities
        .iter()
        .flat_map(|e| foreign_keys(e, &entities))
        .collect();
    if !fks.is_empty() {
        out += &format!("\n-- Foreign keys\n{}\n", fks.join("\n"));
    }
    let idx: Vec<String> = doc.entities.iter().flat_map(indexes).collect();
    if !idx.is_empty() {
        out += &format!("\n-- Indexes\n{}\n", idx.join("\n"));
    }
    out
}

fn column_type(t: &FieldType) -> String {
    match t {
        FieldType::Text | FieldType::Enum(_) => "text".into(),
        FieldType::Int => "bigint".into(),
        FieldType::Float => "double precision".into(),
        FieldType::Bool => "boolean".into(),
        FieldType::Uuid | FieldType::Ref(_) | FieldType::User => "uuid".into(),
        FieldType::Timestamp => "timestamptz".into(),
        FieldType::Date => "date".into(),
        FieldType::Decimal { precision, scale } | FieldType::Money { precision, scale } => {
            format!("numeric({precision}, {scale})")
        }
        FieldType::Json => "jsonb".into(),
    }
}

fn literal(l: &Literal) -> String {
    match l {
        Literal::Bool(b) => b.to_string(),
        Literal::Int(i) => i.to_string(),
        Literal::Float(f) => f.to_string(),
        Literal::Str(s) => lit(s),
    }
}

fn column(f: &Field) -> String {
    let mut col = format!("{} {}", q(&f.name), column_type(&f.ty));
    if f.primary {
        col += " primary key";
        if f.ty == FieldType::Uuid {
            col += " default gen_random_uuid()";
        }
    } else if f.required {
        col += " not null";
    }
    if let Some(d) = &f.default {
        col += &format!(" default {}", literal(d));
    } else if f.auto_now {
        col += " default now()";
    }
    col
}

fn create_table(e: &Entity, entities: &HashMap<&str, &Entity>) -> String {
    let t = snake(&e.name);
    let mut lines: Vec<String> = e.fields.iter().map(column).collect();
    for f in &e.fields {
        if let FieldType::Enum(values) = &f.ty {
            let list: Vec<String> = values.iter().map(|v| lit(v)).collect();
            lines.push(format!(
                "constraint {} check ({} in ({}))",
                q(&format!("{t}_{}_values", f.name)),
                q(&f.name),
                list.join(", ")
            ));
        }
    }
    let cx = Ctx {
        entity: e,
        entities,
    };
    for c in &e.constraints {
        let parsed = expr::parse(&c.expr).expect("validated expression");
        lines.push(format!(
            "constraint {} check ({})",
            q(&c.name),
            expr_sql(&parsed, &cx)
        ));
    }
    let single = e
        .fields
        .iter()
        .filter(|f| f.unique && !f.primary)
        .map(|f| vec![f.name.as_str()]);
    let lists = e
        .unique
        .iter()
        .map(|l| l.fields.iter().map(|x| x.name.as_str()).collect::<Vec<_>>());
    for cols in single.chain(lists) {
        let quoted: Vec<String> = cols.iter().map(|c| q(c)).collect();
        lines.push(format!(
            "constraint {} unique ({})",
            q(&format!("{t}_{}_key", cols.join("_"))),
            quoted.join(", ")
        ));
    }
    format!(
        "create table {} (\n  {}\n);\n",
        table(&e.name),
        lines.join(",\n  ")
    )
}

fn foreign_keys(e: &Entity, entities: &HashMap<&str, &Entity>) -> Vec<String> {
    let t = snake(&e.name);
    e.fields
        .iter()
        .filter_map(|f| {
            let target = match &f.ty {
                FieldType::Ref(r) => {
                    let target = entities.get(r.name.as_str()).expect("validated reference");
                    let pk = target
                        .fields
                        .iter()
                        .find(|x| x.primary)
                        .expect("validated primary key");
                    format!("{} ({})", table(&target.name), q(&pk.name))
                }
                FieldType::User => "\"auth\".\"users\" (\"id\")".into(),
                _ => return None,
            };
            let name = q(&format!("{t}_{}_fkey", f.name));
            Some(format!(
                "alter table {} add constraint {name} foreign key ({}) references {target};",
                table(&e.name),
                q(&f.name)
            ))
        })
        .collect()
}

fn index(e: &Entity, cols: &[&str]) -> String {
    let quoted: Vec<String> = cols.iter().map(|c| q(c)).collect();
    let name = q(&format!("{}_{}_idx", snake(&e.name), cols.join("_")));
    format!(
        "create index {name} on {} ({});",
        table(&e.name),
        quoted.join(", ")
    )
}

/// Declared indexes, then one for each reference field nothing already leads with (SVT010).
fn indexes(e: &Entity) -> Vec<String> {
    let declared = e
        .indexes
        .iter()
        .map(|l| l.fields.iter().map(|x| x.name.as_str()).collect::<Vec<_>>());
    let mut out: Vec<String> = declared.map(|cols| index(e, &cols)).collect();
    for f in &e.fields {
        let leads = |l: &FieldList| l.fields.first().is_some_and(|x| x.name == f.name);
        let covered = f.unique || f.primary || e.unique.iter().chain(&e.indexes).any(leads);
        if matches!(f.ty, FieldType::Ref(_) | FieldType::User) && !covered {
            out.push(index(e, &[f.name.as_str()]));
        }
    }
    out
}
