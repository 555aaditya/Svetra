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
