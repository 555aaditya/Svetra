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

# FIXTURES narrows the run; the rename check at the end needs `teams` to come last.
for name in ${FIXTURES:-finledger posts bookings clinic approvals forum profiles audited jobs triggers validated retries webhooks teams}; do
  docker rm -f svetra-pg-check >/dev/null 2>&1 || true
  docker run -d --name svetra-pg-check -e POSTGRES_PASSWORD=pw "$image" >/dev/null
  # The image runs a temporary server while it initialises, then restarts. Wait for the real one,
  # or a command can land on the temporary server as it shuts down.
  for _ in $(seq 1 90); do docker logs svetra-pg-check 2>&1 | grep -q "init process complete" && break; sleep 1; done
  for _ in $(seq 1 60); do docker exec svetra-pg-check pg_isready -U postgres >/dev/null 2>&1 && break; sleep 1; done
  mkdir -p "$work/$name" && cp "tests/fixtures/$name.yaml" "$work/$name/svetra.yaml"
  # Real hole bodies, where a fixture has them; otherwise compile writes stubs.
  if [ -d "tests/fixtures/holes/$name" ]; then
    mkdir -p "$work/$name/supabase/holes" && cp "tests/fixtures/holes/$name/"*.sql "$work/$name/supabase/holes/"
  fi
  "$bin" compile "$work/$name/svetra.yaml" >/dev/null
  psql() { docker exec -i svetra-pg-check psql -U postgres -v ON_ERROR_STOP=1 -q "$@"; }
  psql < src/standin/base.sql
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

# A declared rename keeps the data (docs/change.md). The last database, teams, still has the rows
# its checks inserted: rename Task.author to creator with `was`, let `svetra plan` write the
# migration, apply it, and every value must still be there under the new name.
renamed="$work/teams-renamed"
mkdir -p "$renamed" && cp -r "$work/teams/." "$renamed/"
sed -e 's/      author: { type: user, required: true }/      creator: { type: user, required: true, was: author }/' \
    -e 's/author == auth.user/creator == auth.user/g' -e 's/      - \[author\]/      - [creator]/' \
    tests/fixtures/teams.yaml > "$renamed/svetra.yaml"
"$bin" plan "$renamed/svetra.yaml" --from tests/fixtures/teams.yaml --at 20260101000000 >/dev/null
before="$(docker exec svetra-pg-check psql -U postgres -At -c 'select count(*) from public.task where author is not null')"
docker exec -i svetra-pg-check psql -U postgres -v ON_ERROR_STOP=1 -q < "$renamed/supabase/migrations/20260101000000_svetra_rename.sql"
after="$(docker exec svetra-pg-check psql -U postgres -At -c 'select count(*) from public.task where creator is not null')"
if [ "$before" -lt 1 ] || [ "$before" != "$after" ]; then echo "rename lost data: $before rows before, $after after"; exit 1; fi
echo "ok: a declared rename keeps every value ($after rows)"

# svetra check (docs/check.md) on a database it didn't design: an app with one of each leak.
# Snapshot it through the printed query, and the findings must match the golden file exactly.
psql -d template1 -c "create database leaky"
# Roles belong to the whole server and already exist, so the stand-in runs without them here.
grep -v "^create role" src/standin/base.sql | docker exec -i svetra-pg-check psql -U postgres -d leaky -v ON_ERROR_STOP=1 -q
docker exec -i svetra-pg-check psql -U postgres -d leaky -v ON_ERROR_STOP=1 -q < tests/pg/check/leaky_app.sql
"$bin" check --print-query | docker exec -i svetra-pg-check psql -U postgres -d leaky -X -A -t -q -v ON_ERROR_STOP=1 > "$work/leaky.json"
if "$bin" check --snapshot "$work/leaky.json" --json > "$work/leaky.check.json"; then echo "check missed the critical leaks"; exit 1; fi
if ! diff "$work/leaky.check.json" tests/golden/leaky_app.check.json; then echo "check findings differ from the golden file"; exit 1; fi
echo "ok: svetra check finds every planted leak ($(grep -c '"kind"' "$work/leaky.check.json") findings)"
