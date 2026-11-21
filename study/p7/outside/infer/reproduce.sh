#!/bin/sh
# Reproduce snapshot.json and draft.yaml from the app's own migrations, then the comparison.
# Run from the repository root after `cargo build`. Needs Docker, curl and python3 with PyYAML.
# Starts one throwaway PostgreSQL 16 container and removes it at the end.
set -eu

C=9964968b53f078bf4e246fc25443e3220a8a143b
BASE=https://raw.githubusercontent.com/supabase/supabase/$C/examples/slack-clone/nextjs-slack-clone/supabase/migrations
SVETRA=${SVETRA:-target/debug/svetra}
OUT=${OUT:-target/outside-infer}
N=svetra-infer-study

mkdir -p "$OUT"
for f in 20240214102356_init.sql 20240214114147_auth-hook.sql; do
  curl -fsSL "$BASE/$f" -o "$OUT/$f"
done

docker rm -f "$N" >/dev/null 2>&1 || true
trap 'docker rm -f "$N" >/dev/null 2>&1 || true' EXIT
docker run -d --name "$N" -e POSTGRES_PASSWORD=pw postgres:16 >/dev/null
sleep 6
until docker exec "$N" pg_isready -q; do sleep 1; done
sleep 2

# The Supabase stand-in (auth.users, auth.uid(), roles), then the two migrations in order.
for f in src/standin/base.sql src/standin/extra.sql "$OUT"/20240214102356_init.sql "$OUT"/20240214114147_auth-hook.sql; do
  docker exec -i "$N" psql -U postgres -v ON_ERROR_STOP=1 -q <"$f" >/dev/null
done

"$SVETRA" infer --print-query >"$OUT/query.sql"
docker exec -i "$N" psql -U postgres -At -f - <"$OUT/query.sql" >"$OUT/snapshot.json"
"$SVETRA" infer --snapshot "$OUT/snapshot.json" --out - >"$OUT/draft.yaml"

mkdir -p "$OUT/compiled"
cp "$OUT/draft.yaml" "$OUT/compiled/svetra.yaml"
"$SVETRA" validate "$OUT/draft.yaml" || true
ABS=$(cd "$(dirname "$SVETRA")" && pwd)/$(basename "$SVETRA")
(cd "$OUT/compiled" && "$ABS" compile svetra.yaml) || true

cmp "$OUT/snapshot.json" study/p7/outside/infer/snapshot.json && echo "snapshot matches the committed one"
cmp "$OUT/draft.yaml" study/p7/outside/infer/draft.yaml && echo "draft matches the committed one"
python3 -I study/p7/outside/infer/compare.py
