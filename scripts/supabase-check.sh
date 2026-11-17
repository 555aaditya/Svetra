#!/usr/bin/env bash
# Run the generated access tests on a real local Supabase, not a stand-in: the database image,
# roles and default privileges Supabase ships. Plain PostgreSQL with a stub missed that Supabase
# grants execute on new functions to anon directly (fixed in 0.4.2); this check is for that kind
# of difference. For each fixture: load its compiled schema with `db reset`, run the tests as
# Supabase's own `postgres` role (not a superuser), then each deliberate breakage, which must fail
# exactly as its .expected file says. Needs Docker and a built `svetra` binary.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
bin="$(cd "$root" && realpath "${SVETRA_BIN:-target/debug/svetra}")"
fixtures="${FIXTURES:-finledger posts bookings clinic teams approvals}"
supabase="${SUPABASE_CLI:-npx -y supabase@3.1.0}"
work="$(mktemp -d)"
project="$work/project"
db="supabase_db_svetra_check"

mkdir -p "$project" && cd "$project"
$supabase init --with-vscode-settings=false --with-intellij-settings=false >/dev/null
sed -i.bak 's/^project_id = .*/project_id = "svetra_check"/' supabase/config.toml && rm supabase/config.toml.bak
mkdir -p supabase/migrations
trap 'cd "$project" && $supabase stop --no-backup >/dev/null 2>&1 || true; rm -rf "$work"' EXIT
# Only the database: the tests talk to Postgres directly, so the API, auth server and the rest
# would only slow the start.
$supabase start -x gotrue,realtime,storage-api,imgproxy,kong,mailpit,postgrest,postgres-meta,studio,edge-runtime,logflare,vector,supavisor >/dev/null

# Run a generated test file. Prints failed checks and fails if any check failed or didn't run.
access_tests() {
  local out
  out="$(docker exec -i "$db" psql -U postgres -d postgres -At < "$1" 2>&1)"
  if grep -E '^(not ok|# Looks like|ERROR)' <<<"$out"; then return 1; fi
}

load() {
  rm -f supabase/migrations/*.sql
  cp "$work/$1/supabase/schemas/svetra.sql" supabase/migrations/20260101000000_svetra.sql
  $supabase db reset >/dev/null 2>&1
}

for name in $fixtures; do
  mkdir -p "$work/$name" && cp "$root/tests/fixtures/$name.yaml" "$work/$name/svetra.yaml"
  if [ -d "$root/tests/fixtures/holes/$name" ]; then
    mkdir -p "$work/$name/supabase/holes" && cp "$root/tests/fixtures/holes/$name/"*.sql "$work/$name/supabase/holes/"
  fi
  "$bin" compile "$work/$name/svetra.yaml" >/dev/null
  tests="$work/$name/supabase/tests/svetra.test.sql"
  load "$name"
  if ! out="$(access_tests "$tests")"; then echo "$out"; exit 1; fi
  for b in "$root"/tests/pg/breakages/"$name".*.sql; do
    [ -e "$b" ] || continue
    what="$(basename "$b" .sql)"
    docker exec -i "$db" psql -U postgres -d postgres -v ON_ERROR_STOP=1 -q < "$b"
    if out="$(access_tests "$tests")"; then echo "access tests missed: $what"; exit 1; fi
    if [ "$out" != "$(cat "${b%.sql}.expected")" ]; then
      echo "unexpected failures for $what:"
      diff <(echo "$out") "${b%.sql}.expected" || true
      exit 1
    fi
    echo "  caught: $what ($(grep -c '^not ok' <<<"$out") checks)"
    load "$name"
  done
  # Svetra's own output, checked by svetra check on a real Supabase: no critical finding.
  "$bin" check --print-query | docker exec -i "$db" psql -U postgres -d postgres -X -A -t -q -v ON_ERROR_STOP=1 > "$work/$name.snapshot.json"
  if ! "$bin" check --snapshot "$work/$name.snapshot.json" > "$work/$name.check.txt"; then
    echo "svetra check reports a critical finding in Svetra's own $name:"; cat "$work/$name.check.txt"; exit 1
  fi
  echo "ok: $name passes its access tests on a real Supabase, and svetra check finds nothing critical"
done
