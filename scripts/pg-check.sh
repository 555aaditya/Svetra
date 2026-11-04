#!/usr/bin/env bash
# Compile each fixture, apply the SQL to a fresh PostgreSQL 16 with a Supabase stand-in,
# and run its hand-written behaviour checks, then the generated access tests (pgTAP).
# Needs Docker and a built `svetra` binary.
set -euo pipefail
cd "$(dirname "$0")/.."
bin="${SVETRA_BIN:-target/debug/svetra}"
image="${PG_IMAGE:-postgres:16}"
work="$(mktemp -d)"
trap 'docker rm -f svetra-pg-check >/dev/null 2>&1 || true; rm -rf "$work"' EXIT

# Run a generated test file. Prints failed checks and fails if any check failed or didn't run.
access_tests() {
  local out
  out="$(docker exec -i svetra-pg-check psql -U postgres -At < "$1" 2>&1)"
  if grep -E '^(not ok|# Looks like|ERROR)' <<<"$out"; then return 1; fi
}

for name in finledger posts bookings; do
  docker rm -f svetra-pg-check >/dev/null 2>&1 || true
  docker run -d --name svetra-pg-check -e POSTGRES_PASSWORD=pw "$image" >/dev/null
  for _ in $(seq 1 60); do docker exec svetra-pg-check pg_isready -U postgres >/dev/null 2>&1 && break; sleep 1; done
  mkdir -p "$work/$name" && cp "tests/fixtures/$name.yaml" "$work/$name/svetra.yaml"
  "$bin" compile "$work/$name/svetra.yaml" >/dev/null
  psql() { docker exec -i svetra-pg-check psql -U postgres -v ON_ERROR_STOP=1 -q "$@"; }
  psql < tests/pg/supabase_stub.sql
  psql < "$work/$name/supabase/schemas/svetra.sql"
  psql < "tests/pg/${name}_assert.sql"
  # The generated tests run after the hand-written checks, whose rows stay behind: existing
  # data must neither break nor hide a result.
  docker exec svetra-pg-check sh -c 'apt-get update -qq && apt-get install -y -qq "postgresql-$PG_MAJOR-pgtap"' >/dev/null 2>&1
  tests="$work/$name/supabase/tests/svetra.test.sql"
  if ! out="$(access_tests "$tests")"; then echo "$out"; exit 1; fi
  if [ "$name" = bookings ]; then
    # A policy added outside svetra.yaml must be caught, with exactly the checks it breaks.
    psql -c 'create policy svetra_ci_leak on public.booking for select to authenticated using (true)'
    if out="$(access_tests "$tests")"; then echo "access tests missed a leaking policy"; exit 1; fi
    want="not ok 57 - alice can't see Booking b
not ok 64 - bob can't see Booking a
not ok 81 - staff at Location a can't see Booking b
# Looks like you failed 3 tests of 87"
    [ "$out" = "$want" ] || { echo "unexpected failures for the leaking policy:"; echo "$out"; exit 1; }
  fi
  echo "ok: $name compiles, applies to $image, and behaves as declared"
done
