//! TypeScript types and Zod validators (docs/compiler.md, TypeScript).
//! The output uses Zod APIs that exist in both Zod 3.23+ and Zod 4.

use crate::model::*;

/// Supabase's API returns `numeric` columns as JSON numbers, which JavaScript rounds past
/// about 15 significant digits. This helper accepts exact strings, and numbers only while
/// they're still exact, and always yields a string. It's emitted only when needed.
const DECIMAL_HELPER: &str = r#"
/** An exact decimal. Accepts a string, or a number only while it's still exact; yields a string. */
const decimal = (precision: number, scale: number) =>
  z
    .union([
      z.string(),
      z.number().refine((n) => Number.isSafeInteger(Math.round(n * 10 ** scale)), {
        message: "Number too large to be exact in JavaScript; select this column as text (column::text).",
      }),
    ])
    .transform(String)
    .pipe(
      z
        .string()
        .regex(
          new RegExp(
            `^-?${precision === scale ? "0" : `\\d{1,${precision - scale}}`}${scale === 0 ? "" : `(\\.\\d{1,${scale}})?`}$`,
          ),
        ),
    );
"#;

pub fn types(doc: &Document) -> String {
    let mut out = String::from("import { z } from \"zod\";\n");
    let has_decimal = doc
        .entities
        .iter()
        .flat_map(|e| &e.fields)
        .any(|f| matches!(f.ty, FieldType::Decimal { .. } | FieldType::Money { .. }));
    if has_decimal {
        out += DECIMAL_HELPER;
    }
    for e in &doc.entities {
        let fields: Vec<String> = e
            .fields
            .iter()
            .map(|f| format!("  {}: {},", f.name, zod(f)))
            .collect();
        out += &format!(
            "\nexport const {name} = z.object({{\n{}\n}});\nexport type {name} = z.infer<typeof {name}>;\n",
            fields.join("\n"),
            name = e.name
        );
    }
    out
}

fn zod(f: &Field) -> String {
    let base = match &f.ty {
        FieldType::Text => "z.string()".to_string(),
        FieldType::Int => "z.number().int()".into(),
        FieldType::Float => "z.number()".into(),
        FieldType::Bool => "z.boolean()".into(),
        FieldType::Uuid | FieldType::Ref(_) | FieldType::User => "z.string().uuid()".into(),
        FieldType::Timestamp => "z.string().datetime({ offset: true })".into(),
        FieldType::Date => "z.string().date()".into(),
        FieldType::Decimal { precision, scale } | FieldType::Money { precision, scale } => {
            format!("decimal({precision}, {scale})")
        }
        FieldType::Enum(values) => {
            let quoted: Vec<String> = values.iter().map(|v| js_string(v)).collect();
            format!("z.enum([{}])", quoted.join(", "))
        }
        FieldType::Json => "z.unknown()".into(),
    };
    if f.required {
        base
    } else {
        format!("{base}.nullable()")
    }
}

/// A double-quoted JavaScript string literal.
fn js_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            c => out.push(c),
        }
    }
    out + "\""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_strings_are_escaped() {
        assert_eq!(js_string("a\"b\\c"), r#""a\"b\\c""#);
    }
}
