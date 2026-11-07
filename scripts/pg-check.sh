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
# Run a generated test file against a database (default: postgres). Prints failed checks and
# fails if any check failed or didn't run.
access_tests() {
  local out
  out="$(docker exec -i svetra-pg-check psql -U postgres -d "${2:-postgres}" -At < "$1" 2>&1)"
  if grep -E '^(not ok|# Looks like|ERROR)' <<<"$out"; then return 1; fi
}

for name in finledger posts bookings clinic; do
  docker rm -f svetra-pg-check >/dev/null 2>&1 || true
  docker run -d --name svetra-pg-check -e POSTGRES_PASSWORD=pw "$image" >/dev/null
  # The image runs a temporary server while it initialises, then restarts. Wait for the real one,
  # or a command can land on the temporary server as it shuts down.
  for _ in $(seq 1 90); do docker logs svetra-pg-check 2>&1 | grep -q "init process complete" && break; sleep 1; done
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
  # Each breakage in tests/pg/breakages/<fixture>.<what>.sql runs on its own copy of the database,
  # and the generated tests must fail exactly as <fixture>.<what>.expected says: proof that they
  # catch it, and that nothing else is reported.
  for b in tests/pg/breakages/"$name".*.sql; do
    [ -e "$b" ] || continue
    what="$(basename "$b" .sql)"
    db="break_${what//[.-]/_}"
    psql -d template1 -c "create database $db template postgres"
    docker exec -i svetra-pg-check psql -U postgres -d "$db" -v ON_ERROR_STOP=1 -q < "$b"
    if out="$(access_tests "$tests" "$db")"; then echo "access tests missed: $what"; exit 1; fi
    if [ "$out" != "$(cat "${b%.sql}.expected")" ]; then
      echo "unexpected failures for $what:"
      diff <(echo "$out") "${b%.sql}.expected" || true
      exit 1
    fi
    echo "  caught: $what ($(grep -c '^not ok' <<<"$out") checks)"
  done
  echo "ok: $name compiles, applies to $image, and behaves as declared"
done
