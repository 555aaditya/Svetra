# Agent evaluation pilot

Does an AI coding agent build a backend that actually enforces who can see and change what, and does having Svetra change that? This folder is the first, small version of the benchmark designed in the project notes (`future/04-agent-ecosystem-and-evaluation.md`). It is a **pilot**: it checks the method works, and it measures nothing yet that should be quoted.

## What it does

For each (prompt, condition, run):

1. Makes a fresh folder and runs headless Claude Code (`claude -p`) on a one-paragraph product request.
2. Takes the database schema the agent produced (`supabase/migrations/`, or Svetra's generated `supabase/schemas/` when the agent used Svetra).
3. Applies it to a real PostgreSQL 16 with the Supabase stand-in (`tests/pg/supabase_stub.sql`), inserts the oracle's test rows, and runs every check in `oracle/<prompt>.json` as real database roles (`anon`, `authenticated`, with the person's id in the claims), never the superuser. Each check is allowed or denied, and is compared with what the prompt's wording requires.
4. Runs `svetra check` on the result for leaks nobody asked about.

A run that fails for an outside reason (a usage limit, a timeout) is marked **invalid** and left out, never scored as a zero.

**Conditions:**

| | What the agent has |
|---|---|
| `alone` | An empty folder |
| `careful` | An empty folder, and the prompt ends with a plain security reminder ("make sure each person can only see and change what they should"). The baseline that separates "any security guidance" from "Svetra" |
| `svetra` | `svetra init` was run: the Svetra skill and MCP server are present, and `svetra` is on the agent's PATH. **The prompt does not mention Svetra**, so this measures whether the agent uses it on its own |

**Prompts:**

| Prompt | What it tests |
|---|---|
| `notes`, `posts` | Easy: a private notes app; a blog with public and private posts (the first pilot) |
| `teams` | Membership: anyone starts a team and joins it, members add people and tasks, authors edit their own tasks. Needs two functions, `start_team` and `add_member`, whose results the checker uses |
| `approvals` | State and columns: only the author edits drafts, a reviewer can approve or reject but change nothing else, nobody edits a decided document. Includes the trap "approve and retitle in one update" |
| `softdelete` | A hidden-after-delete journal: an update that hides the row fails the select policy, so a plain update can't implement it; needs a function, `soft_delete` |
| `rename` | **Two conversation turns:** build the notes app, then (a new session) rename `body` to `content` without losing production data. The checker applies turn one, inserts rows, applies only the new migrations, and checks the rows survived |

**Metrics per run:** checks passed; **leaks** (a check that must be denied and was allowed); **too strict** (a check that must be allowed and was denied); critical findings from `svetra check`; cost, turns and tokens; how many Svetra tools were called.

## Run it

```sh
cargo build
python3 eval/agent/run.py --selftest                       # the checker alone; no agent, no cost
python3 eval/agent/run.py --prompts teams,approvals,softdelete,rename --conditions alone,careful,svetra --seeds 2
python3 eval/agent/run.py --check-only eval/agent/results/<run>   # re-check saved runs
```

Needs Docker, the `claude` CLI logged in, and a built `svetra`. Results go to `eval/agent/results/<timestamp>/` (git-ignored except pilots that are committed on purpose): one folder per run with the agent's files, the full transcript and a `run.json`.

## The checker is tested before the agent is

`--selftest` runs hand-written schemas through the same checker, in plain PostgreSQL. For each prompt a **reference solution** must score 100%, which proves the oracle can be satisfied (and that nothing in it depends on Svetra), and each broken variant must be caught for the right reason: security off, a read policy that is `true`, an insert policy that is `true`, public posts that aren't readable, `add_member` with no membership check, a reviewer who can approve and retitle at once, no way to delete, a rename that drops the column. A benchmark whose checker is wrong measures the checker. The selftest also reproduced a finding from the P7 study by accident: an update policy with `with check (true)` is **not** a leak when a select policy still applies to the updated row.

## Data contract

Each prompt fixes the table and column names (`note(id, owner, body)`), because the checker has to insert and read rows whatever the agent designed. A run whose schema breaks the contract is reported as such, not scored as zero silently. **This is a real limitation:** it favours designs close to plain tables, and it can't test designs where access depends on a representation the prompt doesn't fix (for example team membership, which Svetra stores as role grants and a hand-written schema stores as a table). A fair team-app benchmark needs the setup step to go through the app's own operations, such as an "add member" function, which is future work.

## Result 1: the pilot (2026-11-16, one run per cell, Claude Sonnet 5.5, Claude Code 2.1.319)

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

## Result 2: the hard prompts (2026-11-17, two runs per cell, Claude Sonnet 5.5, Claude Code 2.1.319)

Four harder prompts (`teams`, `approvals`, `softdelete`, `rename`) × three conditions × two runs = 24 runs, **$3.13 in total**. Saved in `results/hard-1/`: the agent's files, full transcripts and a `run.json` per run.

| Prompt | alone | careful | with Svetra |
|---|---|---|---|
| `teams` (23 checks) | 23, 23 · $0.09 · 5 turns | 23, 23 · $0.12 · 8 turns | 23, 23 · $0.21 · 28 turns |
| `approvals` (24) | 24, 24 · $0.11 · 6 turns | 24, 24 · $0.10 · 6 turns | **0**, 24 · $0.20 · 18 turns |
| `softdelete` (17) | 17, 17 · $0.07 · 4 turns | 17, 17 · $0.07 · 5 turns | 17, 17 · $0.18 · 21 turns |
| `rename` (14, two turns) | 14, 14 · $0.11 · 10 turns | 14, 14 · $0.11 · 12 turns | 14, 14 · $0.22 · 32 turns |

(Checks passed in each of the two runs, then mean cost and turns.)

| Over all runs | alone | careful | with Svetra |
|---|---|---|---|
| Runs scoring every check | 8 of 8 | 8 of 8 | 7 of 8 |
| Leaks (should be denied, was allowed) | 0 | 0 | 0 |
| Mean cost per run | $0.09 | $0.10 | $0.20 |
| Mean turns | 6.5 | 7.6 | 24.8 |
| Svetra tools used | n/a | n/a | every run (2 to 9 calls), never prompted |
| Generated access tests left behind | 0 | 0 | 66 on average |

**What it shows:**
- **Even on these prompts, the agent alone wrote correct access control every time:** membership and invitations, a reviewer who may only approve, a delete that hides a row (which an update can't express), a rename that keeps production data. Zero leaks in 16 baseline runs, and the "careful" reminder changed nothing. **The thesis "agents leak data without Svetra" is not supported at this difficulty, with this model.**
- **Svetra cost about twice as much and took about four times as many turns, and was used every time without being asked.** Most of the turns are its own loop (validate, compile, explain, plan), not extra design work.
- **One of eight Svetra runs failed:** the agent put its own `begin … end` around a hole body, which Svetra already wraps, so the migration didn't parse (`approvals`, run 1). The agent had no database to try it on and said so.
- **The approvals prompt pushed Svetra's agents into holes.** The format can't say "an update that changes the status from draft to submitted is allowed" because an update's new-row rule is the same as its old-row rule today, so the agent wrote four hand-written functions. This is the gap that the policy-language research ranked first (split `using` and `check`, and deny rules).
- **Svetra's runs did leave things the baselines don't:** a design file, an explanation, and 38 to 84 generated access tests each. **None of those tests was run here**, because the agent's sandbox has no database, so this harness cannot yet say what they were worth.

**What it does not show:**
- **Not "Svetra doesn't help".** It shows that correctness of what an agent first writes isn't where it helps on prompts like these. The case for Svetra now rests on what the harness doesn't measure: assurance after the fact (tests that run, the access table, the access diff when someone later changes a rule), `check` on apps that already exist, and a human reviewing intent instead of SQL.
- Two runs per cell, one model, one agent, prompts written by the tool's author, and a checker whose reference solutions were written by the same person (they pass, which proves the checks are satisfiable but not that they are the only reasonable reading). Wide uncertainty on every number.
- The harness binary was built before the 0.7.0 version bump (the same code otherwise).

**What to do about it, in order:**
1. **Fix the failure Svetra caused:** `compile` should catch a hole body that opens with its own `begin`, and the skill should say so. (A small, real improvement found by this run.)
2. **Give the agent a database to test on,** so it can run the generated access tests, the way a real user with `supabase start` would. That is where Svetra's loop should pay for itself; right now it pays the cost without the benefit.
3. **Measure what happens later:** a third turn where someone changes a rule (widening access) and the agent must notice the consequence, with and without `plan`.
4. **Harder access patterns, or a weaker model,** to find where baseline agents start to leak. More runs on these prompts would mostly measure cost.

## What the benchmark as a whole cannot show

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
| `selftest/` | Hand-written correct and broken schemas that validate the checker: a reference solution and broken variants for each prompt |
| `results/` | Saved runs: `pilot-1` and `hard-1` are committed on purpose |
