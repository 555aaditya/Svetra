// Webhook form_submitted: checks who is calling, then runs the database function of the same name.
// The header x-form-token must carry the shared secret itself.
// Set the secret with: supabase secrets set FORM_TOKEN=...

const SECRET = "FORM_TOKEN";
const HEADER = "x-form-token";
const RPC = "form_submitted";
const MAX_BYTES = 1_000_000;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const DECIMAL = /^-?\d+(\.\d+)?$/;

// Every input of the hole, and what a valid value of it looks like. All of them are required.
const INPUTS: [string, (v: unknown) => boolean][] = [
  ["invoice", (v: unknown) => typeof v === "string" && UUID.test(v)],
  ["note", (v: unknown) => typeof v === "string"],
];

const bytes = (s: string) => new TextEncoder().encode(s);

// Compares without stopping at the first difference, so how long it takes says nothing about the secret.
function same(a: Uint8Array, b: Uint8Array): boolean {
  let d = a.length ^ b.length;
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) d |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return d === 0;
}

async function signed(body: Uint8Array, secret: string, given: string): Promise<boolean> {
  return same(bytes(secret), bytes(given));
}

const respond = (status: number) => new Response(null, { status });

Deno.serve(async (req: Request) => {
  if (req.method !== "POST") return respond(405);
  const secret = Deno.env.get(SECRET);
  const given = req.headers.get(HEADER);
  const body = new Uint8Array(await req.arrayBuffer());
  if (body.length > MAX_BYTES) return respond(413);
  // Anything wrong with the proof is the same answer, so a caller learns nothing about which part.
  if (!secret || !given || !(await signed(body, secret, given))) return respond(401);

  let json: unknown;
  try {
    json = JSON.parse(new TextDecoder().decode(body));
  } catch {
    return respond(400);
  }
  if (typeof json !== "object" || json === null || Array.isArray(json)) return respond(400);
  const sent = json as Record<string, unknown>;
  const args: Record<string, unknown> = {};
  for (const [name, valid] of INPUTS) {
    if (!(name in sent) || !valid(sent[name])) return respond(400);
    args[name] = sent[name];
  }

  const url = Deno.env.get("SUPABASE_URL");
  const service = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY");
  if (!url || !service) return respond(500);
  const res = await fetch(`${url}/rest/v1/rpc/${RPC}`, {
    method: "POST",
    headers: { apikey: service, Authorization: `Bearer ${service}`, "Content-Type": "application/json" },
    body: JSON.stringify(args),
  });
  if (res.ok) return respond(200);
  // 42501 is the hole's own `when` refusing. Nothing else about a failure leaves this function.
  const code = await res.json().then((e) => e?.code).catch(() => undefined);
  return respond(code === "42501" ? 403 : 500);
});
