# Changelog

## 0.8.1 (2026-11-20)

Fewer false alarms from `svetra check`, found by auditing it on 300 public Supabase repos.

- **`check` explains a policy written for the backend.** When an open policy's name says service role, system or backend and it has no `to service_role`, the finding now says so ("it applies to everyone") and the fix is to delete it, since the service role skips row-level security, or add `to service_role`. Kind and severity are unchanged. It was the most common real open write in the leak-study audit.
- **`check` no longer calls an open insert a critical write.** A permissive `INSERT` policy for `anon` or `public` with a check of `true` lets people add rows, not change existing ones, and is often a sign-up or contact form. It is now a warning, `open_insert` ("Anyone on the internet can add rows to ..."). `open_write` (critical) is now only an open `UPDATE`, `DELETE` or `ALL`. An open insert for signed-in people is ordinary and no longer reported as `signed_in_write`. Found by hand-auditing 30 findings from a study of public Supabase repos: about half of the `open_write` criticals were insert-only.

## 0.8.0 (2026-11-18)

Say what an update may turn a row into, so a state change isn't a hole: `check` on update, and `restrictive` policies.

- **A policy can say which fields an update may change: `only`.** `only: [status]` on a reviewer's policy lets them approve or reject a document and change nothing else, so "approve and retitle in one update" is refused. It is an allow-list, so a column added later stays locked. Svetra generates a guard trigger (the part row-level security can't do, because a policy never sees the old and new row together), refuses with the same error as any policy, and **the access tests prove it on your database**, including the case only the guard can refuse: a locked field changed beside a status change the rules allow. The service role, the owner and custom code are exempt, as they are from row-level security. In the agent evaluation, all four agents given the approvals app wrote this trigger by hand, outside the design; it is now one line. New codes **SVT035** and **SVT036** (a field limited by one policy but freed by another that covers the same person). `explain` says "They can change only its status."
- **A policy can say what an update turns a row into: `check`.** `when` is the row as it is, `check` is the row afterwards (PostgreSQL's `USING` and `WITH CHECK`), so "a draft becomes submitted, a submitted document is approved or rejected, and nothing else" is a policy, not a hand-written function. Without a `check` nothing changes. The access tests try each value a rule with a `check` reads (each enum value, or true and false), so the transitions are proven on your database like the rest. In the agent evaluation, an agent given this exact app wrote four holes; the example (`approvals`) now needs none.
- **`restrictive: true`:** a policy that must hold in addition to a permissive one, compiled to `as restrictive`. "Suspended people can't write" is one rule, not an edit to every policy. `explain` ends each sentence it narrows with "and only when ...". New code **SVT033** (a `check` on a policy that doesn't allow update).
- **`validate` warns when two policies combine into an update neither allows alone (SVT034).** PostgreSQL combines the old-row and new-row conditions independently across policies, so a row can pass one policy's `when` and another's `check`. It found a real flaw in the first draft of the approvals example (a reviewer could make themselves the author), fixed with a constraint.
- **In the test world, a second `user` field points at the other person,** so a rule that compares two people (an author and a reviewer) sees two different ones. No existing example has two `user` fields, so none of their tests change.
- **`compile` refuses a hole body that would break the function around it.** Svetra writes the `begin` and `end` around a hole's hand-written body, so the body must be statements ending in `;`. A body that is a whole function (`begin … end` with no `;` after it) left a stray `end` and a migration that didn't parse; an agent did exactly that in the agent evaluation, with no database to find out. It is now **SVT032**, reported against `supabase/holes/<name>.sql` (over MCP too, as `hole_bodies`), with nothing written. Nested blocks ending in `end;`, trailing comments and empty bodies are still fine. The skill and the agents doc say bodies are statements only.
## 0.7.0 (2026-11-16)

Find the leaks in an app you already have: `svetra check`.

- **`svetra check`: find the leaks in an app you already have.** Point it at any Supabase or PostgreSQL database (`--db URL`, or `--local` for `supabase start`) and it reports, in plain English, where data is open to people who shouldn't reach it: tables with row-level security off, policies that let anyone read or change every row, views and functions that skip row-level security, and rules that trust `user_metadata`. Each finding has a fix. It needs no `svetra.yaml`, runs one read-only query, never prints the password, and exits 1 on a critical finding, so it works in CI. Without `psql`, `--print-query` gives the query to run in the Supabase SQL editor, and `--snapshot` reads its result.
## 0.6.0 (2026-11-16)

Change safely: `svetra diff` and `svetra plan` say what a change does to the data and to who can see what, before it reaches the database.

- **Citable:** a `CITATION.cff`, and every release from now on is archived on Zenodo with a DOI.
- **Safe change: `svetra diff` and `svetra plan`.** They compare `svetra.yaml` with the design the database has (by default, the version committed in git `HEAD`; or `--from`) and classify every change: **safe**, **breaking** (existing rows or app code may fail, such as a new required field with no default) or **destructive** (data is deleted, such as removing a field). They also say whose access widens or narrows, person by person, from the same evaluator as the access tests: "A signed-in person who owns nothing can now see all customers (was: none)." `plan` exits 1 on destructive changes and wider access until they're accepted with `--accept`.
- **Renames keep the data.** Declare one with `was: <old name>` on an entity or field. `plan` writes the `alter ... rename` migration to run before `supabase db diff`, which would otherwise drop the column and add an empty one. An undeclared rename is reported as destructive, with a hint to add `was`. New codes: SVT030 (a `was` that can't be a rename) and SVT031 (a rename that's already applied).
- **Agents:** a read-only `plan` tool in `svetra mcp`, and a skill step: show the person every destructive and wider line, and get their yes before `--accept`.

## 0.5.0 (2026-11-16)

See who can see what: `svetra view`, clearer `explain`, and every release now tested on a real Supabase.

- **Tested on a real Supabase:** CI now runs every example's access tests and deliberate breakages on a real local Supabase, not only on PostgreSQL with a stand-in. That's the check that found the 0.4.2 security bug.
- **`explain` reads fields named for an action as that action:** `paid_by == auth.user` is now "it was paid by them", not "its paid by is them", and `assigned_to` reads "it is assigned to them".
- **`svetra view`: who can see and change what, as one page.** A table of every person (signed out, two signed-in people, one who owns nothing, each role) by every thing and action, answered All, Some or No in words. Select a cell to see which rows the tests tried, the `explain` sentences that allow or rule it out, and the access test numbers that check it. A question box answers "who can see orders?" directly, and the page also shows custom code, a diagram of the links, and sign-in notes. The answers come from the access tests' own evaluator, so the page and the tests can't disagree, and a test checks every cell against the generated test file. It's one self-contained HTML file (`.svetra/view.html`, added to `.gitignore`) with no network access; `--json` prints the view model instead.

## 0.4.2 (2026-11-16)

A security fix for holes, and holes that can give people roles.

- **Security fix: logged-out visitors could call holes meant for signed-in people,** on a real Supabase. Supabase grants execute on every new function in `public` to `anon` directly, and the generated `revoke ... from public` left that grant in place. A hole with a `when` guard still refused them, but a hole without one, such as `write_audit` in the posts example, ran for anyone. Both hole functions now revoke execute from `anon` and `authenticated` by name before granting it to the hole's `as` roles. **Recompile and apply the schema.** The CI stand-in for Supabase now has the same default privileges, so this is checked on every PR; the generated access test `anon can't call <hole>` caught it on a real Supabase.
- **Holes can give people roles,** so people can start a team and invite others. A hole with `grants: [member]` may add and remove grants of the roles it lists, and no other: a row-level security policy on `svetra.role_grants` keeps every other role, such as `admin`, out of its reach. An undeclared role in `grants` is SVT006. `explain`, `context` and `graph` show what each hole can give.
- **Access tests prove it:** each hole's role has privileges on role grants only if it has `grants`, and it can give exactly the roles it lists. A new example app, `teams`, starts teams and adds members through two such holes, and a deliberately widened policy fails exactly one check.

## 0.4.1 (2026-11-13)

Stronger access tests, found by a mutation study, and fixes from running Svetra inside Claude Code.

- **Stronger access tests, found by a mutation study.** `scripts/mutate.py` breaks the deployed database in 197 ways (policies dropped, widened or moved to another role, row-level security off, extra policies, weakened role checks and hole guards, extra grants) and counts what the generated tests catch. They now also check:
  - **carol,** a signed-in person with a second factor who owns nothing and holds no role, so a rule open to every signed-in person, or a role check that forgets who holds the role, fails a check;
  - **changing who owns a row or what it belongs to:** each signed-in person tries to set a row's `user` fields to themselves and point its `ref` fields at another row, and the rule must hold on the row as the update leaves it;
  - **holes:** only the declared callers can run a hole's public function, and nobody can run the function inside `svetra_holes` directly, where the caller's id could be forged;
  - **roles:** alice can't grant herself a role.

  Recompile to get the new checks. The bookings example goes from 87 to 124 tests.
- **`graph` and `context` draw a design that's complete but unsafe.** When the only errors are a table with no rule (SVT001) or money as `float` (SVT005), they still print their output, with the diagnostics on stderr and exit 1; over MCP, the output plus a `diagnostics` list. An agent fixing those errors can see the design instead of drawing it by hand. `explain` still refuses any file with errors.
- **The Claude Code skill re-runs `explain`** before answering "who can see X?", instead of answering from an earlier call.
- **Setup behind a company proxy** is documented: set `NODE_EXTRA_CA_CERTS` to the proxy's root certificate before `npx svetra`, and where the agent is started from, since the MCP server downloads the same way.

## 0.4.0 (2026-11-07)

Svetra inside your agent: holes for custom code, an MCP server, and a Claude Code skill.

- **`svetra init`** sets a project up for agents: the Svetra skill for Claude Code (`.claude/skills/svetra/SKILL.md`), which teaches the design-validate-compile-explain loop and its safety rules; the `svetra` server in `.mcp.json`, keeping every other server; and a starter `svetra.yaml` if there isn't one.
- **`svetra mcp`**, an MCP server over stdio for Claude Code, Cursor, Codex and other agents. Its tools are `validate`, `compile` (with `check`), `explain`, `context` and `graph`, each returning JSON; an invalid file returns its diagnostics with remediation hints. Add it to Claude Code with `claude mcp add svetra -- npx -y svetra mcp`.
- **Holes run as guarded Postgres functions.** A hole like `transfer` compiles to a function the app calls with `supabase.rpc()`. It runs as its own database role, granted only the tables in its `reads` and `writes`. A new `when` condition, such as `source.owner == auth.user`, is checked before any hand-written code and refuses everyone else with SQLSTATE 42501, and a new `as` says who may call it. The body is PL/pgSQL in `supabase/holes/<name>.sql`, created once as a stub and never overwritten. `src/svetra.gen.ts` gets a typed function per hole that validates input and output with Zod.
- **Access tests cover holes:** each hole's role must hold exactly its declared table privileges, and its guard is checked for every person. `explain` says who can call each hole, in plain words.
- **npm package.** `npx svetra` runs the release binary for macOS, Linux or Windows: the first run downloads it from the matching GitHub release, checks its SHA-256 against `SHA256SUMS`, and caches it. No dependencies and no install scripts.

## 0.3.0 (2026-11-05)

Sign-in settings: how people get in, safe by default.

- **Sign-in settings.** A new `auth` section: `sign_in` (email and OAuth providers), `signup` (`open` or `invite_only`), `confirm_email` and `password` rules, with safe defaults (10-character passwords, confirmed emails). `svetra compile` sets only the keys Svetra owns in `supabase/config.toml`, keeping every other line, and `--check` names each key that differs. OAuth secrets are written as `env(...)` references.
- **Second factors for roles.** `mfa: true` on a role makes `has_role` require an `aal2` session. The access tests add a person who holds the role without a second factor and must get nothing extra.
- **`explain`** adds "Signing in" and "Before going live". `context` adds an `auth` line.
- **New checks:** `SVT008` (invalid sign-in settings) and `SVT009` (info: no `auth` section, so Supabase's weak defaults apply).

## 0.2.0 (2026-11-04)

Access tests: your database proves who can see what.

- **Access tests.** `svetra compile` also writes `supabase/tests/svetra.test.sql`, run with `supabase test db`. It checks every person × action × row as real users (`anon` and signed-in, never the service key), with expected results evaluated from `svetra.yaml` rather than the generated SQL. It catches drift, such as a migration adding a permissive policy or turning row-level security off, and reports every failing check, not just the first.
- **Checked on a real local Supabase** (Postgres 17) with `supabase test db`: all 87 bookings checks pass, and a policy added by hand that lets everyone read bookings fails exactly the three checks it breaks. CI runs the same check on PostgreSQL 16 for every PR.

## 0.1.0 (2026-11-04)

The first release. Svetra reads a `svetra.yaml`, checks it, compiles it for Supabase, and explains it.

**Commands**
- `svetra validate [FILE] [--json]` runs 19 checks with stable codes, positions and fixes. Among them:
  - an exposed table with no access rule (SVT001)
  - money stored as float (SVT005)
  - role mistakes (SVT006)
  - logged-out visitors getting more access than signed-in people (SVT007)
  - expressions that don't type-check (SVT003)
- `svetra compile [FILE] [--check]` writes two files:
  - `supabase/schemas/svetra.sql`: tables, constraints, foreign keys, indexes, row-level security on every table, policies, and roles;
  - `src/svetra.gen.ts`: Zod validators and TypeScript types, with exact decimals.

  `--check` fails if either file is stale or hand-edited.
- `svetra explain [FILE]` describes the app in plain English: what's stored, the rules, and who can see and change what.
- `svetra graph [FILE]` draws a Mermaid diagram of entities, links, access, roles and custom code.
- `svetra context [FILE]` gives the whole app in about half the yaml's size, for agents.

**The format** (`docs/format.md`):
- entities, fields and 13 types
- constraints and policies, with `as: authenticated`, `anyone` or `anon`
- holes
- roles: global, or scoped to rows of one entity, with `has_role()`

**Verified:** every change applies the generated SQL to PostgreSQL 16 and reads back after every write, as every role.
