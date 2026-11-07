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
    let entities: std::collections::HashMap<&str, &Entity> =
        doc.entities.iter().map(|e| (e.name.as_str(), e)).collect();
    let inputs: Vec<Entity> = doc
        .holes
        .iter()
        .map(|h| crate::typeck::hole_entity(h, &entities))
        .collect();
    let has_decimal = doc
        .entities
        .iter()
        .chain(&inputs)
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
    if !doc.holes.is_empty() {
        out += HOLES_HEADER;
    }
    for (h, fields) in doc.holes.iter().zip(&inputs) {
        out += &hole(h, fields, &entities);
    }
    out
}

const HOLES_HEADER: &str = r#"
/** The one client method holes need. A Supabase client has it. */
export type Rpc = {
  rpc(fn: string, args: Record<string, unknown>): PromiseLike<{ data: unknown; error: unknown }>;
};
"#;

/// `transfer` -> `Transfer`, `write_audit` -> `WriteAudit`.
fn pascal(snake: &str) -> String {
    snake
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect()
}

/// One typed function per hole: it validates the input, calls the hole, and validates the result.
fn hole(h: &Hole, inputs: &Entity, entities: &std::collections::HashMap<&str, &Entity>) -> String {
    let name = &h.name;
    let mut out = String::new();
    // A hole that returns one value gets a named schema, like its input.
    let single = |out: &mut String, f: Field| {
        let ty = format!("{}Output", pascal(name));
        *out += &format!(
            "\nexport const {ty} = {};\nexport type {ty} = z.infer<typeof {ty}>;\n",
            zod(&f)
        );
        (ty.clone(), Some(format!("{ty}.parse(data)")))
    };
    let (returns, parse) = match &h.output {
        None => ("void".to_string(), None),
        Some(ParamType::Entity(e)) => (e.name.clone(), Some(format!("{}.parse(data)", e.name))),
        Some(ParamType::FieldOf { entity, field }) => {
            let f = entities[entity.name.as_str()]
                .fields
                .iter()
                .find(|f| &f.name == field)
                .expect("validated hole output");
            single(
                &mut out,
                Field {
                    required: true,
                    ..f.clone()
                },
            )
        }
        Some(ParamType::Type(t)) => single(
            &mut out,
            Field {
                name: String::new(),
                pos: h.pos,
                ty: t.clone(),
                required: true,
                primary: false,
                unique: false,
                default: None,
                auto_now: false,
            },
        ),
    };
    let (param, args) = if inputs.fields.is_empty() {
        (String::new(), "{}".to_string())
    } else {
        let ty = format!("{}Input", pascal(name));
        let fields: Vec<String> = inputs
            .fields
            .iter()
            .map(|f| format!("  {}: {},", f.name, zod(f)))
            .collect();
        out += &format!(
            "\nexport const {ty} = z.object({{\n{}\n}});\nexport type {ty} = z.infer<typeof {ty}>;\n",
            fields.join("\n")
        );
        (format!(", input: {ty}"), format!("{ty}.parse(input)"))
    };
    let body = match parse {
        None => format!(
            "  const {{ error }} = await client.rpc({}, {args});\n  if (error) throw error;\n",
            js_string(name)
        ),
        Some(p) => format!(
            "  const {{ data, error }} = await client.rpc({}, {args});\n  if (error) throw error;\n  return {p};\n",
            js_string(name)
        ),
    };
    out += &format!(
        "\n/** Calls the `{name}` hole. Throws the database's error if the call is refused. */\nexport async function {name}(client: Rpc{param}): Promise<{returns}> {{\n{body}}}\n"
    );
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
