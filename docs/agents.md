# Agents: the skill, the MCP server and `svetra init`, version 0.1

This document specifies how coding agents use Svetra: Claude Code, Cursor, Codex and anything else that speaks the Model Context Protocol (MCP). The format is in [`format.md`](format.md) and the outputs are in [`compiler.md`](compiler.md). When the code and this document disagree, that's a bug in one of them.

The model, from the project's vision: **you design the data with your agent, Svetra makes it safe, and the agent builds the rest.** The agent edits `svetra.yaml` and writes hole bodies. Svetra checks every edit, compiles everything else, and explains the result in plain English. The agent never writes SQL policies, grants or types by hand.

## `svetra init`

```sh
svetra init [DIR]
```

Sets a project up for agents. `DIR` defaults to the current folder. It writes:

| File | What | If it exists |
|---|---|---|
| `.claude/skills/svetra/SKILL.md` | The Svetra skill (below) | Replaced, so it stays current. It's generated and marked as such |
| `.mcp.json` | Adds a `svetra` server: `{"command": "npx", "args": ["-y", "svetra", "mcp"]}`. `-y` matters: agents start MCP servers without a terminal, so npx's install prompt would hang | The `svetra` entry is added or updated, and every other server and key is kept, in order. Invalid JSON is refused, not overwritten |
| `svetra.yaml` | A valid starting file: a `Note` that only its author can see and change, and an `auth` section with email sign-in | **Never overwritten** |

`init` prints what it wrote and the next step: "Open your agent and describe the app you want."

**Behind a company proxy** that inspects HTTPS, `npx` can't download the binary until `NODE_EXTRA_CA_CERTS` points at the proxy's root certificate. Set it before `init`, and in the terminal the agent is started from: the agent starts `svetra mcp` with its own environment, so an agent opened another way (for example from an editor) needs the variable too, or the server fails to start.

## The skill

`SKILL.md` teaches the agent one loop, plus the rules that keep it safe. Its content is fixed by this section:

1. **Design first.** Turn the person's request into `svetra.yaml`: entities, fields, policies, roles, `auth`, and holes only for logic that can't be a policy. Ask at most three questions, each with a recommended default. A new design is a new file; a change to an existing one goes through `patch` with the `revision` from `context` (a `conflict` means read again and redo), and every destructive and wider line in its answer goes to the person before going on. A `rename` doesn't rewrite references to the old name: `set` each one in the same call.
2. **Validate after every edit,** with the `validate` tool, or `svetra validate --json`. Fix every error using its `remediation.hint` before going on. Don't move on with errors.
3. **Plan a change,** when `svetra.yaml` already exists in git: call `plan`. Show the person every destructive line and every wider-access line, in their words, and get an explicit yes before running `svetra plan --accept <kinds>`. Declare renames with `was:`, never by removing and adding.
4. **Compile,** with the `compile` tool. Never edit the generated files (`supabase/schemas/svetra.sql`, `src/svetra.gen.ts`, `supabase/tests/svetra.test.sql`), or the keys Svetra owns in `supabase/config.toml`.
5. **Write only hole bodies,** in `supabase/holes/<name>.sql`, as statements only: Svetra writes the `begin` and `end` around them, so never wrap the body in its own `begin … end`, and end every statement with `;` (`compile` refuses a body that doesn't, with SVT032). Use the `p_` inputs, touch only the declared entities, and never add a check that `when` already makes. For code that runs on a clock, use `kind: schedule` (see [format.md](format.md#scheduled-holes)): no caller, `p_caller` is `null`, and the body is safe to run twice and to run late. For code that must run when a row changes, use `kind: trigger` with `on`, `events` and optionally `when` (see [format.md](format.md#triggered-holes)): the body gets the row as `p_old` and `p_new`, runs in the same transaction as the change, and a body that raises undoes the person's change.
6. **Explain,** with the `explain` tool. Show the person the "Who can see and change what" section, and "Before going live" if there is one, in their words, not SQL.
7. **Prove it,** with `supabase test db` when a local Supabase is running. Report failures as they are, and never weaken a policy to make a test pass.

And these rules:
- **Never** use the service role key in app code, and never disable row-level security.
- **Prefer a policy over a hole,** and a hole over app code, for anything about who can see or change data.
- **Use `context`** to read the whole design compactly, instead of opening the generated files.
- **When the person asks "who can see X?",** call `explain` again and answer from its lines, even if it ran earlier in the conversation. Never answer from memory: the file may have changed.

## The MCP server

```sh
svetra mcp
```

An MCP server over stdio: JSON-RPC 2.0, one message per line. It supports `initialize`, `tools/list` and `tools/call`, and answers `ping`. Every tool except `check` and `doctor` takes a `file` (default `svetra.yaml`, relative to the server's working directory) and returns JSON text the agent can act on, never terminal formatting.

| Tool | Does | Returns |
|---|---|---|
| `validate` | `svetra validate --json` | The diagnostics JSON, exactly as the CLI prints it |
| `compile` | `svetra compile`, or `--check` with `check: true` | `{"written": [...], "notes": [...]}`, or `{"stale": [...]}` with `check`. On errors, the diagnostics |
| `explain` | `svetra explain` | `{"text": "..."}` |
| `context` | `svetra context` | `{"text": "...", "revision": "..."}` (`explain` and `graph` carry the `revision` too) |
| `graph` | `svetra graph` | `{"mermaid": "..."}` |
| `plan` | `svetra plan --json`, read-only: compares `file` with `from`, or with `file` as committed in git `HEAD` | The plan: each change's `kind`, `data` (safe, breaking, destructive), `access` (wider, narrower), message, hint and `because`; `needs_yes`; `renames`. Never writes the rename migration: that's `svetra plan --accept`, after the person agrees |
| `patch` | `svetra patch --json`: checked `set`, `remove` and `rename` edits on dot-separated paths, all or nothing, with `expected_revision` and `dry_run` (see [patch.md](patch.md)) | `{"applied", "revision", "problems", "notes", "changes", "needs_yes"}`, or `{"conflict": true, ...}`. Refusals and conflicts are results; only input that can't be read is a tool error |
| `check` | `svetra check --json`: what is open in an existing database or in migrations (see below) | The findings JSON as the CLI prints it (with a `migrations` object for migrations), plus a `summary` |
| `doctor` | `svetra doctor --json`: the pre-launch checklist for a project folder (see below) | The doctor JSON as the CLI prints it (`findings`, `counts`, `reminders`, `not_checked`, `ready`, and `migrations` for migrations), plus a `summary` |

`explain` returns the diagnostics as a tool error when the file has errors. `context` and `graph` do too, except when the only errors are SVT001 or SVT005: then they return their output plus a `diagnostics` list, so the agent can see the design while it fixes it.

### The `check` tool

Answers "what is open in this database, or in these migrations?" for apps built with or without Svetra. It needs no `svetra.yaml`, and never writes. Inputs, exactly one of the first four:

| Input | Is | Same as |
|---|---|---|
| `db` | a connection string, read through `psql` | `--db URL` |
| `local` | `true` for the database `supabase start` runs | `--local` |
| `snapshot` | path to a saved snapshot JSON, relative to the server's working directory | `--snapshot FILE` |
| `migrations` | a folder of migrations, or `true` for `supabase/migrations` | `--migrations DIR` |
| `scratch` | with `migrations`: an empty database to build them in, instead of Docker | `--scratch URL` |
| `strict` | with `migrations`: also fail when any migration could not be applied | `--strict` |
| `schemas` | the schemas the API exposes. Default `["public"]` | `--schema` |

The result is the JSON of `svetra check --json` (`findings` and `counts`, plus `migrations` for migrations) with one more key, `summary`: a sentence with the counts, such as "2 critical, 3 warnings, 1 note in public (the database)", and for migrations how many applied. Passwords in a connection string are never echoed: `summary` and every error name the database only through the same redaction as the CLI. Findings never make the call an error: the agent reads `counts`. The result is a tool error, with one sentence saying what to do, when the call can't run: no source or more than one, `scratch` or `strict` without `migrations`, an unreadable snapshot, a database or Docker that can't be reached, or (with `strict`) a migration that failed to apply. Nothing is exit-coded: where the CLI exits 1 for a critical finding, the tool returns it as a normal result.

### The `doctor` tool

Answers "is this project ready to launch?" for apps built with or without Svetra: `check`'s findings on a database, plus leaked keys, auth settings, public storage buckets, generated files and backups, read from the project folder ([doctor.md](doctor.md)). It needs no `svetra.yaml`, never writes and makes no network call. Inputs, all optional:

| Input | Is | Same as |
|---|---|---|
| `dir` | the project folder, relative to the server's working directory. Default `.` | `DIR` |
| `db` | a connection string, read through `psql` | `--db URL` |
| `local` | `true` for the database `supabase start` runs | `--local` |
| `migrations` | a folder of migrations, or `true` for `dir/supabase/migrations` | `--migrations [DIR]` |
| `scratch` | with `migrations`: an empty database to build them in, instead of Docker | `--scratch URL` |
| `strict` | with `migrations`: also fail when any migration could not be applied | `--strict` |
| `schemas` | the schemas the API exposes. Default `["public"]` | `--schema` |

At most one of `db`, `local` and `migrations`. With none, the database is listed under `not_checked`. The result is the JSON of `svetra doctor --json` with one more key, `summary`: one sentence with the counts and the verdict ("1 critical, 2 warnings, 0 notes. Fix the 1 critical finding first. Not ready."). Passwords are never echoed, and no secret's value is in any finding. Findings never make the call an error: the agent reads `counts` and `ready`, which is true only for a full, clean run. The result is a tool error, with one sentence, when the call can't run: more than one source, `scratch` or `strict` without `migrations`, `dir` not a folder, a database or Docker that can't be reached, or (with `strict`) a migration that failed to apply.

- **No editing tools:** agents already edit files. Svetra's job is to check and compile what they wrote, and keeping it read-and-compile only keeps the server small and safe.
- **Errors:** a tool that can't run, such as a missing file, returns an MCP tool error with one sentence saying what to do, never a stack trace.
- **Protocol version:** the server answers with the version the client asks for if it supports it, and its own otherwise. Version 0.1 supports `2025-06-18`.

## Not in version 0.1

- Tools that edit `svetra.yaml` directly.
- Setup files for other agents. Claude Code reads `SKILL.md` and the project's `.mcp.json`. Cursor (`.cursor/mcp.json` and rules) and Codex (its `config.toml` and `AGENTS.md`) can run `svetra mcp` today if added by hand; `init` writing their files comes later.
