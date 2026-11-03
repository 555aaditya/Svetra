//! Fields and types (SVT023, SVT029).

use super::{Cx, pos, show};
use crate::diag::Code;
use crate::model::*;
use crate::yaml::{Node, Value};

const FIELD_KEYS: &[&str] = &[
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

impl Cx {
    pub(super) fn field(&mut self, name: &Node, body: &Node, parent: &str) -> Option<Field> {
        let fname = self.string(name, parent)?;
        let path = format!("{parent}.{fname}");
        let entries = self.map(body, &path)?;
        let k = self.keys(entries, &path, FIELD_KEYS);
        let ty = self.field_type(&k, &path, name)?;
        let flag = |cx: &mut Cx, key: &str| {
            k.get(key)
                .is_some_and(|v| cx.boolean(v, &format!("{path}.{key}")))
        };
        let primary = flag(self, "primary");
        let mut field = Field {
            name: fname,
            pos: pos(name),
            required: primary || flag(self, "required"),
            primary,
            unique: flag(self, "unique"),
            default: None,
            auto_now: false,
            ty,
        };
        if let Some(v) = k.get("default") {
            field.default = self.default(v, &field.ty, &format!("{path}.default"));
        }
        if let Some(v) = k.get("auto") {
            let apath = format!("{path}.auto");
            if !matches!(field.ty, FieldType::Timestamp | FieldType::Date) {
                self.report(
                    Code::Svt023,
                    &apath,
                    v,
                    "`auto` applies only to timestamp and date fields.".into(),
                    "Remove `auto`, or change the type.",
                );
            } else if self.string(v, &apath).as_deref() == Some("now") {
                field.auto_now = true;
            } else if v.as_str().is_some() {
                self.report(
                    Code::Svt023,
                    &apath,
                    v,
                    "`auto` must be `now`.".into(),
                    "Write auto: now.",
                );
            }
        }
        Some(field)
    }

    /// Read `type` and the options it needs. Options that don't apply to the type are SVT023.
    pub(super) fn field_type(
        &mut self,
        k: &std::collections::HashMap<&str, &Node>,
        path: &str,
        at: &Node,
    ) -> Option<FieldType> {
        let tnode = self.need(k, "type", path, at)?;
        let tpath = format!("{path}.type");
        let tname = self.string(tnode, &tpath)?;
        let uses: &[&str] = match tname.as_str() {
            "decimal" | "money" => &["precision", "scale"],
            "enum" => &["values"],
            "ref" => &["entity"],
            _ => &[],
        };
        for opt in ["values", "precision", "scale", "entity"] {
            if let (Some(v), false) = (k.get(opt), uses.contains(&opt)) {
                let msg = format!("`{opt}` does not apply to type `{tname}`.");
                self.report(
                    Code::Svt023,
                    &format!("{path}.{opt}"),
                    v,
                    msg,
                    &format!("Remove `{opt}`."),
                );
            }
        }
        let ty = match tname.as_str() {
            "text" => FieldType::Text,
            "int" => FieldType::Int,
            "float" => FieldType::Float,
            "bool" => FieldType::Bool,
            "uuid" => FieldType::Uuid,
            "timestamp" => FieldType::Timestamp,
            "date" => FieldType::Date,
            "json" => FieldType::Json,
            "user" => FieldType::User,
            "decimal" | "money" => {
                let (precision, scale) = self.precision(k, path, at)?;
                if tname == "money" {
                    FieldType::Money { precision, scale }
                } else {
                    FieldType::Decimal { precision, scale }
                }
            }
            "enum" => FieldType::Enum(self.enum_values(k, path, at)?),
            "ref" => {
                let e = self.need(k, "entity", path, at)?;
                FieldType::Ref(NameRef {
                    name: self.string(e, &format!("{path}.entity"))?,
                    pos: pos(e),
                })
            }
            other => {
                let msg = format!("Unknown type `{other}`.");
                let hint = "Use one of: text, int, float, bool, uuid, timestamp, date, decimal, money, enum, json, ref, user.";
                self.report(Code::Svt023, &tpath, tnode, msg, hint);
                return None;
            }
        };
        Some(ty)
    }

    fn precision(
        &mut self,
        k: &std::collections::HashMap<&str, &Node>,
        path: &str,
        at: &Node,
    ) -> Option<(u8, u8)> {
        let p = self.need(k, "precision", path, at);
        let s = self.need(k, "scale", path, at);
        let (p, s) = (p?, s?);
        match (&p.value, &s.value) {
            (Value::Int(pv), Value::Int(sv)) if (1..=38).contains(pv) && (0..=*pv).contains(sv) => {
                Some((*pv as u8, *sv as u8))
            }
            _ => {
                let msg = "Precision must be 1 to 38, and scale 0 to the precision.".to_string();
                self.report(
                    Code::Svt023,
                    &format!("{path}.precision"),
                    p,
                    msg,
                    "For money, precision: 18, scale: 6 is a safe default.",
                );
                None
            }
        }
    }

    fn enum_values(
        &mut self,
        k: &std::collections::HashMap<&str, &Node>,
        path: &str,
        at: &Node,
    ) -> Option<Vec<String>> {
        let v = self.need(k, "values", path, at)?;
        let vpath = format!("{path}.values");
        let values: Vec<String> = self
            .name_list(v, &vpath)
            .into_iter()
            .map(|n| n.name)
            .collect();
        let mut seen = std::collections::HashSet::new();
        if values.is_empty() || !values.iter().all(|x| seen.insert(x)) {
            self.report(
                Code::Svt023,
                &vpath,
                v,
                "Enum values must be a non-empty list with no repeats.".into(),
                "List each value once.",
            );
            return None;
        }
        Some(values)
    }

    fn default(&mut self, v: &Node, ty: &FieldType, path: &str) -> Option<Literal> {
        let lit = match &v.value {
            Value::Bool(b) => Literal::Bool(*b),
            Value::Int(i) => Literal::Int(*i),
            Value::Float(f) => Literal::Float(*f),
            Value::Str(s) => Literal::Str(s.clone()),
            _ => {
                let msg = format!("A default must be a single value, found a {}.", v.kind());
                self.report(Code::Svt029, path, v, msg, "Write one literal value.");
                return None;
            }
        };
        let ok = match (ty, &lit) {
            (FieldType::Ref(_) | FieldType::User, _) => false,
            (FieldType::Enum(values), Literal::Str(s)) => values.contains(s),
            (
                FieldType::Text
                | FieldType::Uuid
                | FieldType::Timestamp
                | FieldType::Date
                | FieldType::Json,
                Literal::Str(_),
            ) => true,
            (FieldType::Bool, Literal::Bool(_)) => true,
            (FieldType::Int, Literal::Int(_)) => true,
            (
                FieldType::Float | FieldType::Decimal { .. } | FieldType::Money { .. },
                Literal::Int(_) | Literal::Float(_),
            ) => true,
            _ => false,
        };
        if !ok {
            let msg = format!(
                "This default does not fit the field's type at {}.",
                show(path)
            );
            self.report(
                Code::Svt023,
                path,
                v,
                msg,
                "Use a value of the field's type, or remove the default.",
            );
            return None;
        }
        Some(lit)
    }
}
