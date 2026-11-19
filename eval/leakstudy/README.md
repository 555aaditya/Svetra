# Leak study: `svetra check` on open-source Supabase apps

**Question:** among open-source apps built on Supabase, what share has an access leak that `svetra check` reports, and of what kinds? This folder is the harness. It holds code only: no repo names, no clones, no findings (see `.gitignore`). The plan, limits and ethics are in the author's notes; the rules that matter are repeated here.

## Rules this harness follows

- **Public repositories only.** It never connects to anyone's deployed database and never uses a key found in a repository. It only reads `*.sql` files under `supabase/migrations/`, and it never runs any other file from a clone.
- **Migrations run in a throwaway container** with no network (`--network none`), dropped capabilities, and memory, process and time limits. The container is deleted afterwards, and so is every clone.
- **Aggregates only.** Results are keyed by a salted hash. Don't publish or commit repo names. If a finding looks serious and the app appears live, tell the maintainer privately first.

## What it does

```
find     search GitHub for repos with supabase/migrations, filter, write data/candidates.json
draw     take a seeded random sample of N from the candidates (per group)
run      for each repo: clone, apply migrations, run `svetra check`, write one row, delete everything
report   aggregate data/results.csv into the numbers for the write-up
selftest check the harness itself on a local leaky app and a clean one (no network)
```

Needs Docker, the `gh` CLI logged in (for `find` and `draw` metadata), `psql`, and a built `svetra`. Standard library only.

## The numbers it reports

Applied rate; leak rate (applied repos with at least one critical finding); findings by kind; AI-built against other; the hand audit sample for false positives. Migrations are not production, so a repo's migrations can leak when the live app doesn't. The write-up must say so.

## The Supabase stand-in, and what it can't do

Migrations assume Supabase. The database starts from `tests/pg/supabase_stub.sql` plus `stub_extra.sql` (the service role, the `extensions` and `storage` schemas, `auth.role()`, the realtime publication, Supabase's search path). It is not Supabase: extensions that the stock PostgreSQL image lacks (`pg_net`, `pg_cron`, `vector`) fail, and so does any migration that depends on a table made in the dashboard. A file that fails is rolled back whole, as `supabase db reset` runs each one, and counted. A repo is `clean` if every file applied, `partial` if some did, `none` if none did. Spot-check ten repos on a real local Supabase before trusting a number.

## Results (run 2026-11-18, 300 random public repos, `svetra` 0.8.0 plus #79)

Aggregates only; no repo, owner or table is named anywhere.

| | Repos |
|---|---|
| Drawn (seed 2, from 1,711 found) | 300 |
| Skipped, too large | 25 |
| No migration applied | 12 |
| Some applied / all applied | 189 / 74 |
| **Checked** (at least one applied) | **263** |
| With a critical finding | **68 (26%)** |
| With a critical finding, among the 74 where every migration applied | **9 (12%)** |

Critical kinds, repos affected: `rls_off` 46, `open_write` 37, `view_bypass` 1. The first run (before #79) read 97 (37%): about half the `open_write` criticals were insert-only policies, which let people add rows but not change them and are often a contact form. A hand audit of 30 findings found that, `check` was fixed (open insert is now the warning `open_insert`), and the same 300 were re-run. A second audit of 30 (22 reachable) found no false positives, and 8 repos re-checked on a real local Supabase gave identical critical findings.

What this is not: a rate of leaking apps. These are migrations, not live databases; a table can be fixed in the dashboard or by a script that is not a migration, and partly applied repos can hide fixes in files that failed (the main failure was `relation does not exist`, 456 times). Public repos with SQL migrations are not a random sample of apps. The AI-built grouping is guessed from files and packages and the groups are small (28% against 20%, not a difference to claim). Of the real open writes in the audit, most were policies meant for the service role that, with no `to service_role`, apply to everyone.

## Pilot (10 repos, seed 1), what it taught about the harness

This was to fix the harness, not a result: ten repos prove nothing about leak rates.

- First run: no repo applied cleanly, because the stand-in lacked the service role, the `storage` schema and the search path. Adding them (`stub_extra.sql`) took it to 4 clean, 4 partial, 1 with nothing applied, 1 skipped as too large.
- What still fails is mostly `relation does not exist`: migrations that build on tables made outside them. That is the main limit on "applied cleanly", and the full run should report it next to every rate.
- A clone that times out, or a repo that breaks the harness, becomes a row and no longer stops the run.

## Running it

```sh
cargo build
python3 eval/leakstudy/study.py selftest            # no network
python3 eval/leakstudy/study.py find --pages 3      # about 15 minutes; writes data/candidates.json
python3 eval/leakstudy/study.py draw --n 10 --seed 1
python3 eval/leakstudy/study.py run --seed 1        # resumable; one row per repo in data/results.jsonl
python3 eval/leakstudy/study.py report
```

Details for the hand audit (which objects `check` flagged) stay in `data/findings/`, which is git-ignored.
