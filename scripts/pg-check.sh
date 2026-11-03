#!/usr/bin/env bash
# Compile each fixture, apply the SQL to a fresh PostgreSQL 16 with a Supabase stand-in,
# and run its behaviour checks. Needs Docker and a built `svetra` binary.
set -euo pipefail
cd "$(dirname "$0")/.."
bin="${SVETRA_BIN:-target/debug/svetra}"
image="${PG_IMAGE:-postgres:16}"
work="$(mktemp -d)"
trap 'docker rm -f svetra-pg-check >/dev/null 2>&1 || true; rm -rf "$work"' EXIT

for name in finledger posts; do
  docker rm -f svetra-pg-check >/dev/null 2>&1 || true
  docker run -d --name svetra-pg-check -e POSTGRES_PASSWORD=pw "$image" >/dev/null
  for _ in $(seq 1 60); do docker exec svetra-pg-check pg_isready -U postgres >/dev/null 2>&1 && break; sleep 1; done
  mkdir -p "$work/$name" && cp "tests/fixtures/$name.yaml" "$work/$name/svetra.yaml"
  "$bin" compile "$work/$name/svetra.yaml" >/dev/null
  psql() { docker exec -i svetra-pg-check psql -U postgres -v ON_ERROR_STOP=1 -q "$@"; }
  psql < tests/pg/supabase_stub.sql
  psql < "$work/$name/supabase/schemas/svetra.sql"
  psql < "tests/pg/${name}_assert.sql"
  echo "ok: $name compiles, applies to $image, and behaves as declared"
done
