#!/usr/bin/env bash
# Call the generated Edge Functions of the `webhooks` fixture over HTTP, on a real local Supabase with
# its REST API and Edge Runtime: the part of a webhook that plain PostgreSQL can't run. A signed call
# does its work, a repeat is ignored, and every wrong call is refused with the status it should get:
# no signature, a wrong one, a wrong secret, a body of the wrong shape, a refusing `when`, a failing
# `require`. The calls carry no Supabase token, so this also shows `verify_jwt = false` was written.
# Needs Docker, Node, openssl, python3 and a built `svetra` binary.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
bin="$(cd "$root" && realpath "${SVETRA_BIN:-target/debug/svetra}")"
supabase="${SUPABASE_CLI:-npx -y supabase@3.1.0}"
work="$(mktemp -d)"
project="$work/project"
db="supabase_db_svetra_hooks"
api="http://127.0.0.1:54321"
hmac_secret="whsec-for-the-test"
form_token="form-token-for-the-test"

mkdir -p "$project" && cd "$project"
$supabase init --with-vscode-settings=false --with-intellij-settings=false >/dev/null
sed -i.bak 's/^project_id = .*/project_id = "svetra_hooks"/' supabase/config.toml && rm supabase/config.toml.bak
trap 'cd "$project" && $supabase stop --no-backup >/dev/null 2>&1 || true; rm -rf "$work"' EXIT

# The design, its hole bodies and the generated files, exactly as a user's folder would have them.
cp "$root/tests/fixtures/webhooks.yaml" svetra.yaml
mkdir -p supabase/holes supabase/migrations
cp "$root/tests/fixtures/holes/webhooks/"*.sql supabase/holes/
"$bin" compile svetra.yaml >/dev/null
cp supabase/schemas/svetra.sql supabase/migrations/20260101000000_svetra.sql
# The secrets, as `supabase secrets set` would hold them on a hosted project.
printf 'PAYMENTS_WEBHOOK_SECRET=%s\nFORM_TOKEN=%s\n' "$hmac_secret" "$form_token" > supabase/functions/.env
grep -q 'functions.payment_received' supabase/config.toml || { echo "compile didn't write verify_jwt for payment_received"; exit 1; }

$supabase start -x studio,logflare,vector,mailpit,imgproxy,storage-api,realtime,supavisor,postgres-meta,gotrue >/dev/null

sql() { docker exec -i "$db" psql -U postgres -d postgres -X -A -t -q -v ON_ERROR_STOP=1 -c "$1"; }
fail=0
# expect <status> <what> <curl args...>: the call's HTTP status must be the one expected.
expect() {
  local want="$1" what="$2"; shift 2
  local got
  got="$(curl -s -o /dev/null -m 60 -w '%{http_code}' "$@")"
  if [ "$got" = "$want" ]; then echo "  ok: $what ($got)"; else echo "  FAILED: $what: wanted $want, got $got"; fail=1; fi
}
# check <what> <sql> <want>: a count read from the database.
check() {
  local got; got="$(sql "$2")"
  if [ "$got" = "$3" ]; then echo "  ok: $1 ($got)"; else echo "  FAILED: $1: wanted $3, got $got"; fail=1; fi
}
sign() { printf '%s' "$1" | openssl dgst -sha256 -hmac "$hmac_secret" -hex | sed 's/^.* //'; }

sql "insert into auth.users (id) values ('a0000000-0000-0000-0000-000000000000') on conflict do nothing; insert into public.invoice (id, owner, amount) values ('11111111-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 50);"
fn="$api/functions/v1"
inv='11111111-0000-0000-0000-000000000000'
good="{\"invoice\":\"$inv\",\"paid\":50,\"event_id\":\"evt-1\"}"

echo "payment_received (hmac_sha256, dedupe, when, audit)"
expect 200 "a correctly signed call, with no Supabase token" -X POST "$fn/payment_received" -H "x-signature: $(sign "$good")" -d "$good"
check "it made one payment" "select count(*) from public.payment" 1
check "it marked the invoice paid" "select count(*) from public.invoice where status = 'paid'" 1
check "it was audited, with no actor" "select count(*) from svetra.audit_log where subject = 'payment_received' and actor is null" 1
expect 200 "the same event again is answered" -X POST "$fn/payment_received" -H "x-signature: $(sign "$good")" -d "$good"
check "and ignored: still one payment" "select count(*) from public.payment" 1
expect 401 "no signature header" -X POST "$fn/payment_received" -d "$good"
expect 401 "a wrong signature" -X POST "$fn/payment_received" -H "x-signature: $(printf '0%.0s' $(seq 64))" -d "$good"
expect 401 "a signature made with another secret" -X POST "$fn/payment_received" -H "x-signature: $(printf '%s' "$good" | openssl dgst -sha256 -hmac other -hex | sed 's/^.* //')" -d "$good"
tampered="${good/50/5000}"
expect 401 "a signature that doesn't match a changed body" -X POST "$fn/payment_received" -H "x-signature: $(sign "$good")" -d "$tampered"
expect 405 "a GET" "$fn/payment_received"
bad='{"invoice":"not-a-uuid","paid":50,"event_id":"evt-2"}'
expect 400 "a signed body with a bad uuid" -X POST "$fn/payment_received" -H "x-signature: $(sign "$bad")" -d "$bad"
missing="{\"invoice\":\"$inv\",\"event_id\":\"evt-2\"}"
expect 400 "a signed body with an input missing" -X POST "$fn/payment_received" -H "x-signature: $(sign "$missing")" -d "$missing"
notjson='this is not json'
expect 400 "a signed body that isn't JSON" -X POST "$fn/payment_received" -H "x-signature: $(sign "$notjson")" -d "$notjson"
zero="{\"invoice\":\"$inv\",\"paid\":0,\"event_id\":\"evt-3\"}"
expect 403 "a signed call the hole's when refuses" -X POST "$fn/payment_received" -H "x-signature: $(sign "$zero")" -d "$zero"
check "a refused call left no event behind, so a retry can run" "select count(*) from svetra.webhook_events where event_id = 'evt-3'" 0
gone="{\"invoice\":\"99999999-0000-0000-0000-000000000000\",\"paid\":5,\"event_id\":\"evt-4\"}"
expect 500 "a signed call whose body fails, with no detail" -X POST "$fn/payment_received" -H "x-signature: $(sign "$gone")" -d "$gone"
check "a failed call left no event behind either" "select count(*) from svetra.webhook_events where event_id = 'evt-4'" 0

echo "form_submitted (shared_secret in x-form-token, require)"
form="{\"invoice\":\"$inv\",\"note\":\"hello\"}"
expect 200 "the right shared secret" -X POST "$fn/form_submitted" -H "x-form-token: $form_token" -d "$form"
expect 401 "no token" -X POST "$fn/form_submitted" -d "$form"
expect 401 "a wrong token" -X POST "$fn/form_submitted" -H "x-form-token: nope" -d "$form"
empty="{\"invoice\":\"$inv\",\"note\":\"\"}"
expect 400 "a call that breaks the hole's require" -X POST "$fn/form_submitted" -H "x-form-token: $form_token" -d "$empty"

echo "the database function behind it"
# Mint the project's anon and service keys from its documented local JWT secret.
keys="$(python3 - <<'PY'
import hmac, hashlib, base64, json, time
def b(x): return base64.urlsafe_b64encode(x).rstrip(b"=")
def jwt(role):
    h = b(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    p = b(json.dumps({"iss": "supabase-demo", "role": role, "exp": int(time.time()) + 86400}).encode())
    s = b(hmac.new(b"super-secret-jwt-token-with-at-least-32-characters-long", h + b"." + p, hashlib.sha256).digest())
    return (h + b"." + p + b"." + s).decode()
print(jwt("anon"), jwt("service_role"))
PY
)"
anon="${keys% *}"
rpc="{\"invoice\":\"$inv\",\"paid\":50,\"event_id\":\"evt-9\"}"
expect 401 "the anon key can't call it directly" -X POST "$api/rest/v1/rpc/payment_received" -H "apikey: $anon" -H "Authorization: Bearer $anon" -H "Content-Type: application/json" -d "$rpc"

[ "$fail" = 0 ] || { echo "webhook-check: some calls got the wrong answer"; exit 1; }
echo "ok: the webhook functions check who is calling, and reach the database only through the service key"
