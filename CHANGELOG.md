# Changelog

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
