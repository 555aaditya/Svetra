//! The Edge Function in front of a webhook hole (docs/compiler.md, Webhook holes). It is generated
//! in full: it reads the raw body, checks the signature in constant time, checks the inputs'
//! types, and calls the hole's database function with the service key. It imports nothing, so a
//! call never waits on the network to start, and it holds no logic of the app's own.

use crate::model::*;
use std::collections::HashMap;

/// The file a webhook's Edge Function is written to, relative to the folder of `svetra.yaml`.
pub fn path(hole: &str) -> String {
    format!("supabase/functions/{hole}/index.ts")
}

/// A JSON string literal, which is also a valid TypeScript one.
fn js(s: &str) -> String {
    serde_json::to_string(s).expect("a string is valid JSON")
}

/// A condition on `v` that holds when it is a valid value of this type.
fn check(t: &FieldType) -> String {
    match t {
        FieldType::Text => "typeof v === \"string\"".into(),
        FieldType::Enum(values) => {
            let list: Vec<String> = values.iter().map(|v| js(v)).collect();
            format!("typeof v === \"string\" && [{}].includes(v)", list.join(", "))
        }
        FieldType::Int => "typeof v === \"number\" && Number.isInteger(v)".into(),
        FieldType::Float => "typeof v === \"number\" && Number.isFinite(v)".into(),
        FieldType::Bool => "typeof v === \"boolean\"".into(),
        FieldType::Uuid | FieldType::Ref(_) | FieldType::User => {
            "typeof v === \"string\" && UUID.test(v)".into()
        }
        FieldType::Timestamp => "typeof v === \"string\" && !Number.isNaN(Date.parse(v))".into(),
        FieldType::Date => {
            "typeof v === \"string\" && DATE.test(v) && !Number.isNaN(Date.parse(v))".into()
        }
        FieldType::Decimal { .. } | FieldType::Money { .. } => {
            "(typeof v === \"string\" && DECIMAL.test(v)) || (typeof v === \"number\" && Number.isFinite(v))"
                .into()
        }
        // The one type with no shape to check: anything, including null, is a json value.
        FieldType::Json => "true".into(),
    }
}

/// The Edge Function of one webhook hole.
pub fn function(h: &Hole, w: &Webhook, entities: &HashMap<&str, &Entity>, header: &str) -> String {
    let inputs = crate::typeck::hole_entity(h, entities);
    let fields: Vec<String> = inputs
        .fields
        .iter()
        .map(|f| format!("  [{}, (v: unknown) => {}],", js(&f.name), check(&f.ty)))
        .collect();
    let proof = match w.scheme {
        Scheme::HmacSha256 => {
            "  const key = await crypto.subtle.importKey(\"raw\", bytes(secret), { name: \"HMAC\", hash: \"SHA-256\" }, false, [\"sign\"]);\n  const mac = new Uint8Array(await crypto.subtle.sign(\"HMAC\", key, body));\n  const hex = Array.from(mac, (b) => b.toString(16).padStart(2, \"0\")).join(\"\");\n  return same(bytes(hex), bytes(given.trim().toLowerCase()));"
        }
        Scheme::SharedSecret => "  return same(bytes(secret), bytes(given));",
    };
    let how = match w.scheme {
        Scheme::HmacSha256 => "the hex HMAC-SHA256 of the raw body, keyed by the secret",
        Scheme::SharedSecret => "the shared secret itself",
    };
    format!(
        r#"// {header}
// Webhook {name}: checks who is calling, then runs the database function of the same name.
// The header {hdr} must carry {how}.
// Set the secret with: supabase secrets set {secret}=...

const SECRET = {secret_js};
const HEADER = {hdr_js};
const RPC = {name_js};
const MAX_BYTES = 1_000_000;
const UUID = /^[0-9a-f]{{8}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{4}}-[0-9a-f]{{12}}$/i;
const DATE = /^\d{{4}}-\d{{2}}-\d{{2}}$/;
const DECIMAL = /^-?\d+(\.\d+)?$/;

// Every input of the hole, and what a valid value of it looks like. All of them are required.
const INPUTS: [string, (v: unknown) => boolean][] = [
{fields}
];

const bytes = (s: string) => new TextEncoder().encode(s);

// Compares without stopping at the first difference, so how long it takes says nothing about the secret.
function same(a: Uint8Array, b: Uint8Array): boolean {{
  let d = a.length ^ b.length;
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) d |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return d === 0;
}}

async function signed(body: Uint8Array, secret: string, given: string): Promise<boolean> {{
{proof}
}}

const respond = (status: number) => new Response(null, {{ status }});

Deno.serve(async (req: Request) => {{
  if (req.method !== "POST") return respond(405);
  const secret = Deno.env.get(SECRET);
  const given = req.headers.get(HEADER);
  const body = new Uint8Array(await req.arrayBuffer());
  if (body.length > MAX_BYTES) return respond(413);
  // Anything wrong with the proof is the same answer, so a caller learns nothing about which part.
  if (!secret || !given || !(await signed(body, secret, given))) return respond(401);

  let json: unknown;
  try {{
    json = JSON.parse(new TextDecoder().decode(body));
  }} catch {{
    return respond(400);
  }}
  if (typeof json !== "object" || json === null || Array.isArray(json)) return respond(400);
  const sent = json as Record<string, unknown>;
  const args: Record<string, unknown> = {{}};
  for (const [name, valid] of INPUTS) {{
    if (!(name in sent) || !valid(sent[name])) return respond(400);
    args[name] = sent[name];
  }}

  const url = Deno.env.get("SUPABASE_URL");
  const service = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  if (!url || !service) return respond(500);
  const res = await fetch(`${{url}}/rest/v1/rpc/${{RPC}}`, {{
    method: "POST",
    headers: {{ apikey: service, Authorization: `Bearer ${{service}}`, "Content-Type": "application/json" }},
    body: JSON.stringify(args),
  }});
  if (res.ok) return respond(200);
  // 42501 is the hole's own `when` refusing. Nothing else about a failure leaves this function.
  const code = await res.json().then((e) => e?.code).catch(() => undefined);
  return respond(code === "42501" ? 403 : 500);
}});
"#,
        header = header,
        name = h.name,
        name_js = js(&h.name),
        hdr = w.header,
        hdr_js = js(&w.header),
        secret = w.secret,
        secret_js = js(&w.secret),
        how = how,
        fields = fields.join("\n"),
        proof = proof,
    )
}
