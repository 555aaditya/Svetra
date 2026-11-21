//! `svetra infer` (docs/infer.md): turn a catalog snapshot into a draft `svetra.yaml`.
//! Entities and fields only; every part that can't be translated stays as an `UNTRANSLATED`
//! comment with the original SQL.

use crate::check::MONEY_WORDS;
use crate::compile::names::snake;
use serde_json::Value;

/// Is this a name Svetra accepts for a field, constraint or table: `^[a-z][a-z0-9_]*$`?
pub fn is_snake(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The entity for a table: its name in PascalCase, or `None` when that wouldn't compile back to
/// the same table (docs/infer.md, Naming). No singularizing: `order_items` is `OrderItems`.
pub fn entity_name(table: &str) -> Option<String> {
    if !is_snake(table) {
        return None;
    }
    let name: String = table
        .split('_')
        .map(|part| {
            let mut c = part.chars();
            match c.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    (name.starts_with(|c: char| c.is_ascii_uppercase()) && snake(&name) == table).then_some(name)
}

/// What a Svetra field type is, for reading a default value.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Text,
    Enum(Vec<String>),
    Int,
    Float,
    Decimal,
    Bool,
    Date,
    Timestamp,
    Uuid,
    Json,
}

/// A column's type in Svetra's terms (docs/infer.md, Mapping).
#[derive(Debug, Clone, PartialEq)]
pub struct Mapped {
    /// The `type:` value.
    pub ty: &'static str,
    /// Extra keys that belong with the type, such as `values: [a, b]`, each as `key: value`.
    pub opts: Vec<String>,
    pub kind: Kind,
    /// Set when the compiled column is not exactly the original: says how.
    pub lossy: Option<String>,
}

fn mapped(ty: &'static str, kind: Kind) -> Mapped {
    Mapped {
        ty,
        opts: Vec::new(),
        kind,
        lossy: None,
    }
}

fn lossy(mut m: Mapped, why: &str) -> Mapped {
    m.lossy = Some(why.to_string());
    m
}

/// A YAML scalar for a string: plain when that is unambiguous, otherwise single-quoted.
pub fn scalar(s: &str) -> String {
    let plain = s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && !matches!(
            s.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n" | "inf" | "nan"
        );
    if plain {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "''"))
    }
}

/// The type of a column, or why it can't be translated. `column` is the column's snapshot row
/// (`type`, `kind`, `type_schema`, `type_name`) and `enums` is the snapshot's `enums` rows.
pub fn map_type(column: &Value, enums: &[&Value]) -> Result<Mapped, String> {
    let s = |k: &str| column[k].as_str().unwrap_or_default();
    let name = s("name");
    let money_like = name.split('_').any(|w| MONEY_WORDS.contains(&w));
    let full = s("type").to_ascii_lowercase();
    match s("kind") {
        "e" => {
            let found = enums
                .iter()
                .find(|e| e["schema"] == s("type_schema") && e["name"] == s("type_name"));
            let Some(e) = found else {
                return Err(format!("enum type {} is not in the snapshot", s("type")));
            };
            let values: Vec<String> = e["values"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if values.is_empty() {
                return Err("enum type with no values".into());
            }
            let list: Vec<String> = values.iter().map(|v| scalar(v)).collect();
            return Ok(lossy(
                Mapped {
                    ty: "enum",
                    opts: vec![format!("values: [{}]", list.join(", "))],
                    kind: Kind::Enum(values),
                    lossy: None,
                },
                &format!(
                    "was enum type {}, compiles to text with a check",
                    s("type_name")
                ),
            ));
        }
        "b" => {}
        other => {
            return Err(format!(
                "type {} ({})",
                s("type"),
                match other {
                    "d" => "a domain",
                    "c" => "a composite type",
                    "r" | "m" => "a range type",
                    _ => "not a base type",
                }
            ));
        }
    }
    // Arrays and extension types have a base kind too; they fall through to the error below.
    let base = full
        .split('(')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let m = match base.as_str() {
        "uuid" => mapped("uuid", Kind::Uuid),
        "text" => mapped("text", Kind::Text),
        "character varying" | "character" | "varchar" | "char" if full.contains('(') => lossy(
            mapped("text", Kind::Text),
            &format!("was {full}, the length limit is dropped"),
        ),
        "character varying" | "character" => mapped("text", Kind::Text),
        "citext" => lossy(
            mapped("text", Kind::Text),
            "was citext, comparisons become case-sensitive",
        ),
        "boolean" => mapped("bool", Kind::Bool),
        "date" => mapped("date", Kind::Date),
        // `timestamp(3) with time zone` has its precision inside the name.
        _ if full.starts_with("timestamp") && full.ends_with("with time zone") => {
            mapped("timestamp", Kind::Timestamp)
        }
        _ if full.starts_with("timestamp") && full.ends_with("without time zone") => lossy(
            mapped("timestamp", Kind::Timestamp),
            "was timestamp without time zone, compiles to timestamptz",
        ),
        "jsonb" => mapped("json", Kind::Json),
        "json" => lossy(mapped("json", Kind::Json), "was json, compiles to jsonb"),
        "smallint" | "integer" | "bigint" => {
            if base == "bigint" {
                mapped("int", Kind::Int)
            } else {
                lossy(
                    mapped("int", Kind::Int),
                    &format!("was {base}, compiles to bigint"),
                )
            }
        }
        "real" | "double precision" => {
            if money_like {
                return Err(format!(
                    "{full} named like money; Svetra refuses float for money (SVT005), choose numeric(p,s)"
                ));
            }
            if base == "real" {
                lossy(
                    mapped("float", Kind::Float),
                    "was real, compiles to double precision",
                )
            } else {
                mapped("float", Kind::Float)
            }
        }
        "numeric" => {
            let args = full
                .split_once('(')
                .and_then(|(_, rest)| rest.strip_suffix(')'));
            let Some(args) = args else {
                return Err("numeric without precision and scale".into());
            };
            let (p, sc) = args.split_once(',').unwrap_or((args, "0"));
            let (Ok(p), Ok(sc)) = (p.trim().parse::<u32>(), sc.trim().parse::<u32>()) else {
                return Err(format!("{full} can't be read"));
            };
            if p == 0 || p > 38 || sc > p {
                return Err(format!(
                    "{full}: Svetra allows 1 to 38 digits and a scale up to the precision"
                ));
            }
            Mapped {
                ty: if money_like { "money" } else { "decimal" },
                opts: vec![format!("precision: {p}"), format!("scale: {sc}")],
                kind: Kind::Decimal,
                lossy: None,
            }
        }
        _ => return Err(format!("type {}", s("type"))),
    };
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn col(name: &str, ty: &str) -> Value {
        json!({ "name": name, "type": ty, "kind": "b", "type_schema": "pg_catalog", "type_name": "x" })
    }

    fn ty(name: &str, t: &str) -> Result<(String, Vec<String>, bool), String> {
        map_type(&col(name, t), &[]).map(|m| (m.ty.to_string(), m.opts, m.lossy.is_some()))
    }

    fn exact(t: &str) -> String {
        let (ty, opts, lossy) = ty("x", t).unwrap();
        assert!(opts.is_empty() && !lossy, "{t} should be exact");
        ty
    }

    #[test]
    fn exact_types() {
        assert_eq!(exact("uuid"), "uuid");
        assert_eq!(exact("text"), "text");
        assert_eq!(exact("boolean"), "bool");
        assert_eq!(exact("date"), "date");
        assert_eq!(exact("timestamp with time zone"), "timestamp");
        assert_eq!(exact("timestamp(3) with time zone"), "timestamp");
        assert_eq!(exact("jsonb"), "json");
        assert_eq!(exact("bigint"), "int");
        assert_eq!(exact("double precision"), "float");
    }

    #[test]
    fn widened_types_say_so() {
        for t in [
            "integer",
            "smallint",
            "real",
            "character varying(255)",
            "character(3)",
            "citext",
            "json",
            "timestamp without time zone",
        ] {
            assert!(ty("x", t).unwrap().2, "{t} should carry a lossy note");
        }
        assert_eq!(ty("x", "integer").unwrap().0, "int");
        assert_eq!(ty("x", "character varying").unwrap().0, "text");
        assert!(
            !ty("x", "character varying").unwrap().2,
            "no limit, no loss"
        );
    }

    #[test]
    fn numeric_is_decimal_or_money_by_name() {
        let (t, o, _) = ty("quantity", "numeric(10,3)").unwrap();
        assert_eq!(
            (t.as_str(), o),
            ("decimal", vec!["precision: 10".into(), "scale: 3".into()])
        );
        assert_eq!(ty("price", "numeric(12,2)").unwrap().0, "money");
        assert_eq!(ty("unit_price", "numeric(12,2)").unwrap().0, "money");
        assert_eq!(ty("priceless", "numeric(12,2)").unwrap().0, "decimal");
        assert_eq!(ty("n", "numeric(5)").unwrap().1[1], "scale: 0");
    }

    #[test]
    fn what_it_cannot_translate_is_an_error_not_a_guess() {
        assert!(ty("rating", "numeric").is_err());
        assert!(ty("rating", "numeric(40,2)").is_err());
        assert!(ty("tags", "text[]").is_err());
        assert!(ty("blob", "bytea").is_err());
        assert!(ty("ip", "inet").is_err());
        assert!(
            ty("total", "double precision").is_err(),
            "float money is SVT005"
        );
        assert!(ty("weight", "real").is_ok());
        let mut domain = col("email", "email_address");
        domain["kind"] = json!("d");
        assert!(map_type(&domain, &[]).is_err());
    }

    #[test]
    fn enums_keep_their_values() {
        let mut c = col("status", "order_status");
        c["kind"] = json!("e");
        c["type_schema"] = json!("public");
        c["type_name"] = json!("order_status");
        let e =
            json!({ "schema": "public", "name": "order_status", "values": ["open", "on", "it's"] });
        let m = map_type(&c, &[&e]).unwrap();
        assert_eq!(m.ty, "enum");
        assert_eq!(m.opts, vec!["values: [open, 'on', 'it''s']"]);
        assert!(m.lossy.is_some());
        assert!(map_type(&c, &[]).is_err(), "an enum the snapshot lacks");
    }

    #[test]
    fn entity_names_compile_back_to_the_table() {
        assert_eq!(entity_name("order_items").as_deref(), Some("OrderItems"));
        assert_eq!(entity_name("profiles").as_deref(), Some("Profiles"));
        assert_eq!(entity_name("t2_x").as_deref(), Some("T2X"));
        for bad in ["Order", "a__b", "_x", "2fa", "t_2x", "my-table", ""] {
            assert_eq!(entity_name(bad), None, "{bad}");
        }
    }
}
