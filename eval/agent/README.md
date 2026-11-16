# Agent evaluation pilot

Does an AI coding agent build a backend that actually enforces who can see and change what, and does having Svetra change that? This folder is the first, small version of the benchmark designed in the project notes (`future/04-agent-ecosystem-and-evaluation.md`). It is a **pilot**: it checks the method works, and it measures nothing yet that should be quoted.

## What it does

For each (prompt, condition, run):

1. Makes a fresh folder and runs headless Claude Code (`claude -p`) on a one-paragraph product request.
2. Takes the database schema the agent produced (`supabase/migrations/`, or Svetra's generated `supabase/schemas/` when the agent used Svetra).
3. Applies it to a real PostgreSQL 16 with the Supabase stand-in (`tests/pg/supabase_stub.sql`), inserts the oracle's test rows, and runs every check in `oracle/<prompt>.json` as real database roles (`anon`, `authenticated`, with the person's id in the claims), never the superuser. Each check is allowed or denied, and is compared with what the prompt's wording requires.
4. Runs `svetra check` on the result for leaks nobody asked about.

**Conditions:**

| | What the agent has |
|---|---|
| `alone` | An empty folder |
| `svetra` | `svetra init` was run: the Svetra skill and MCP server are present. **The prompt does not mention Svetra**, so this measures whether the agent uses it on its own |

**Metrics per run:** checks passed; **leaks** (a check that must be denied and was allowed); **too strict** (a check that must be allowed and was denied); critical findings from `svetra check`; cost, turns and tokens; how many Svetra tools were called.

## Run it

```sh
cargo build
python3 eval/agent/run.py --selftest                       # the checker alone; no agent, no cost
python3 eval/agent/run.py --prompts notes,posts --conditions alone,svetra --seeds 1 --budget 3
python3 eval/agent/run.py --check-only eval/agent/results/<run>   # re-check saved runs
```

Needs Docker, the `claude` CLI logged in, and a built `svetra`. Results go to `eval/agent/results/<timestamp>/` (git-ignored except pilots that are committed on purpose): one folder per run with the agent's files, the full transcript and a `run.json`.

## The checker is tested before the agent is

`--selftest` runs hand-written schemas through the same checker: a correct schema must score 100%, and each broken one (security off, a read policy that is `true`, an insert policy that is `true`, a blog whose public posts aren't readable) must be caught for the right reason. A benchmark whose checker is wrong measures the checker. The selftest also reproduced a finding from the P7 study by accident: an update policy with `with check (true)` is **not** a leak when a select policy still applies to the updated row.

## Data contract

Each prompt fixes the table and column names (`note(id, owner, body)`), because the checker has to insert and read rows whatever the agent designed. A run whose schema breaks the contract is reported as such, not scored as zero silently. **This is a real limitation:** it favours designs close to plain tables, and it can't test designs where access depends on a representation the prompt doesn't fix (for example team membership, which Svetra stores as role grants and a hand-written schema stores as a table). A fair team-app benchmark needs the setup step to go through the app's own operations, such as an "add member" function, which is future work.

## Pilot result (2026-11-16, one run per cell, Claude Sonnet 5.5, Claude Code 2.1.319)

| run | oracle checks | leaks | `svetra check` critical | cost | turns | output tokens | Svetra tools used |
|---|---|---|---|---|---|---|---|
| notes, alone | 12/12 | 0 | 0 | $0.06 | 5 | 1,221 | 0 |
| notes, with Svetra | 12/12 | 0 | 0 | $0.10 | 12 | 1,877 | 2 |
| blog, alone | 15/15 | 0 | 0 | $0.05 | 4 | 1,505 | 0 |
| blog, with Svetra | 15/15 | 0 | 0 | $0.13 | 18 | 2,874 | 5 |

Saved in `results/pilot-1/` (the agent's files, full transcripts and `run.json` for each run).

**What it shows:**
- **The harness works.** The checker is validated by `--selftest`; every run applied, was scored by real database roles, and produced a comparable row. Costs and turns come straight from Claude Code.
- **With Svetra installed and the prompt silent about it, the agent used it every time:** it loaded the skill, called `validate` and `compile`, and in the blog run also `explain` and `plan`, then built its migration from Svetra's generated schema.

**What it does not show, and the uncomfortable part:**
- **On these two simple prompts the agent alone was already correct,** with no leaks, at roughly half the cost and a third of the turns. So **Svetra did not improve correctness here, and it cost more.** That is one run per cell, so it is not a result to quote either way, but it is the expected direction: a current model writes correct row-level security for "each person sees their own rows" and "public or private posts". The thesis that agents leak data was never going to show on a notes app.
- **What Svetra adds is not measured by this harness yet:** the agent alone ships no tests and no plain-English explanation, while the Svetra runs also produced generated access tests (`supabase/tests/`) and an `explain`. A fair comparison has to score those, and has to include the harder bands (roles and tenants, rules like "an author cannot approve their own post", changes such as renames) and the second turn of a conversation, where safe change matters.
- **So the next step is the hard prompts, not more seeds on easy ones.** Running the 20-prompt design on simple apps would mostly measure cost.

## What this pilot cannot show

- **No statistics.** One run per cell. Agents vary run to run; the plan is at least 5 per cell, and bootstrap intervals, before any claim.
- **Two prompts, both simple.** Single-owner notes and a blog with public and private posts. The full design has 20 across four difficulty bands, including roles and tenants.
- **One agent and one model** (Claude Code, pinned by `--model`). Not a claim about other agents.
- **The user's global `CLAUDE.md` still loads** into every headless run even with `--setting-sources project`. It is the same in both conditions, but it is not a clean room.
- **The oracle author is the tool's author.** The checks follow directly from each prompt's sentences, and are published so others can check them, but an outside reviewer should read them before any result is quoted.
- **The stand-in is not Supabase.** The generated schemas have passed on a real Supabase elsewhere, but this harness uses the stand-in for speed. An agent that relies on Supabase-specific features could behave differently.
- **A baseline that knows security matters is missing.** The full design adds a condition where the prompt includes a plain security reminder, so any gain from Svetra isn't just "the agent was told to be careful".

## Files

| Path | What |
|---|---|
| `run.py` | The runner and checker |
| `prompts/` | The product requests, one file each |
| `oracle/` | The people, test rows and allowed or denied checks for each prompt |
| `selftest/` | Hand-written correct and broken schemas that validate the checker |
| `results/` | Saved runs |
