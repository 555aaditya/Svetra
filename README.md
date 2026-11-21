<div align="center">

# Svetra

**Design the system with your AI. Svetra compiles it to the low level.**

One small file, `svetra.yaml`, compiles to a locked-down Supabase backend,
and writes the tests that prove it on your real database.

[![CI](https://github.com/555aaditya/Svetra/actions/workflows/ci.yml/badge.svg)](https://github.com/555aaditya/Svetra/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/555aaditya/Svetra?color=2ea88a)](https://github.com/555aaditya/Svetra/releases)
[![npm](https://img.shields.io/npm/v/svetra?color=cb3837)](https://www.npmjs.com/package/svetra)
[![crates.io](https://img.shields.io/crates/v/svetra?color=e6892f)](https://crates.io/crates/svetra)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.24262588.svg)](https://doi.org/10.5281/zenodo.24262588)

[**Website**](https://555aaditya.github.io/Svetra/) ·
[Install](#install) ·
[Format](docs/format.md) ·
[Compiler](docs/compiler.md) ·
[Changelog](CHANGELOG.md) ·
[Roadmap](#roadmap)

```sh
npx svetra --version
```

</div>

---

People now build apps by talking to AI agents, and most of them can't read the code those agents write. That code decides the things that matter most: what data exists, which rules always hold, and who can see what. When it's wrong, users' data leaks. The most common case is a database table that anyone on the internet can read.

Svetra moves those decisions out of the code and into one small, typed file, `svetra.yaml`. Your agent writes and edits it while you design the app together. Svetra checks every change, and will compile it into the database schema, security policies, types and validators, deterministically, the same way every time. You review a short file and a plain-English explanation instead of thousands of generated lines, and your agent spends fewer tokens because it stops rewriting boilerplate.

> **Status: early, and in use.** Version 0.8 of the tool. The format is `svetra: "0.1"`. Every release is tested on PostgreSQL 16 and on a real Supabase. See the [roadmap](#roadmap).

## What it looks like

```yaml
svetra: "0.1"
system: FinLedger

entities:
  Account:
    fields:
      id:      { type: uuid, primary: true }
      owner:   { type: user, required: true }
      balance: { type: money, precision: 18, scale: 6, required: true }
    constraints:
      balance_non_negative: "balance >= 0"

policies:
  owner_reads_own_accounts:
    on: Account
    allow: [select]
    when: "owner == auth.user"

holes:
  transfer:
    kind: function
    input: { source: Account.id, destination: Account.id, amount: Account.balance }
    writes: [Account]
```

Every table is closed by default. Here, people can read only their own accounts, and balances change only through `transfer`, which is a *hole*: the one place where custom code goes.

## Install

On macOS or Linux, this downloads a release and checks its SHA-256:

```sh
curl -fsSL https://raw.githubusercontent.com/555aaditya/Svetra/main/install.sh | sh
```

Or with Node 18+, on macOS, Linux or Windows. The npm package downloads the same release binary on first run, checks its SHA-256, and caches it:

```sh
npx svetra --version              # run it without installing
npm install -g svetra             # or put `svetra` on your PATH
npm install --save-dev svetra     # or pin it per project, then `npx svetra ...`
```

**Behind a company proxy** that inspects HTTPS, such as Zscaler, the first run can't download the binary and says `unable to get local issuer certificate`. Point Node at the proxy's root certificate first, in the same terminal you start your agent from, because the MCP server downloads the same way:

```sh
export NODE_EXTRA_CA_CERTS=/path/to/proxy-root.pem
```

Your IT team can give you the certificate. On macOS it's usually in the System keychain: `security find-certificate -c "<proxy name>" -p /Library/Keychains/System.keychain > ~/proxy-root.pem`.

Or with Rust 1.88+: `cargo install svetra`. Windows builds are also on the [releases page](https://github.com/555aaditya/Svetra/releases).

## Quick start

```sh
svetra init                      # skill, MCP server and a starter svetra.yaml, for Claude Code
svetra validate svetra.yaml      # every problem at once, with a fix for each
svetra compile svetra.yaml       # SQL, TypeScript and Zod, and the access tests
supabase test db                 # the tests, run on your database as real users
```

## Commands

| Command | What it does |
|---|---|
| `validate` | Checks `svetra.yaml`; positions, stable codes, fixes |
| `compile` | Writes the schema, row-level security, types and access tests |
| `explain`, `context`, `graph` | Your app in plain English, compact context for agents, a diagram |
| `view` | One page: who can see and change what |
| `diff`, `plan` | Classifies a change as safe, breaking or destructive, and keeps renamed data |
| `check` | Finds leaks in a database you already have, no `svetra.yaml` needed |
| `new` | Starts a project from a template (notes, blog, bookings, teams, approvals); `new --list` describes them ([spec](docs/new.md)) |
| `infer` | Drafts a `svetra.yaml` (entities, fields and owner-column and `true` policies, with `check`'s findings as comments) from a database you already have, never wider than the database; whatever it can't translate stays an `UNTRANSLATED` comment. `infer --check-draft` runs the draft's access tests on a throwaway copy of the original database and lists where they disagree ([spec](docs/infer.md)) |
| `doctor` | The pre-launch checklist for a project folder: service keys and other secrets in code, committed `.env` files, secrets in `supabase/config.toml`, generated files out of date. Never prints a secret's value, never says "ready" while the database is unchecked ([spec](docs/doctor.md)) |
| `mcp`, `init` | Serves all of this to your agent; sets up a project |

## What works today: `svetra validate`

```sh
svetra validate svetra.yaml          # readable output
svetra validate svetra.yaml --json   # for agents
```

It reports every problem at once, each with an exact position, a stable code and a fix:

```
svetra.yaml:4:3: error SVT001: Order is exposed through the API but has no policy.
  hint: Add a policy on Order, or mark it private: true.
svetra.yaml:7:7: error SVT005: `total` looks like money but uses float, which loses cents.
  hint: Use { type: money, precision: 18, scale: 6 }.
```

There are 28 checks. They cover tables with no access policy, weak sign-in settings, money stored as `float`, references to things that don't exist, policy and constraint expressions that don't type-check, enum typos, and more. The exit code is `0` for valid, `1` for errors and `2` for a usage error.

## What works today: `svetra compile`

```sh
svetra compile svetra.yaml          # writes the files below, next to svetra.yaml
svetra compile svetra.yaml --check  # writes nothing; fails if they are stale or hand-edited
```

| File | What's in it |
|---|---|
| `supabase/schemas/svetra.sql` | Tables, constraints, foreign keys and indexes; row-level security on every table; one policy per action. Supabase's `supabase db diff` turns it into migrations |
| `src/svetra.gen.ts` | One Zod validator and TypeScript type per entity. Money stays exact: it's a string, never a float |
| `supabase/tests/svetra.test.sql` | Access tests for `supabase test db`: every person × action × row, run as real signed-in and logged-out users, never the service key. Every failure is reported, and nothing persists |

**The access tests check your database, not just the files.** If another migration adds a policy that lets everyone read bookings, `supabase test db` fails with `alice can't see Booking b` and every other check that broke.

Roles work too: declare `admin`, or `staff` scoped to one location, and use `has_role('staff', location)` in a rule. `explain` then says "People with the staff role for a booking's location can see and change that booking."

**Built for your agent.** `svetra mcp` serves `validate`, `compile`, `explain`, `context` and `graph` to any MCP agent, with JSON results it can act on. For Claude Code, `svetra init` sets up a project in one step: the Svetra skill in `.claude/skills/`, the MCP server in `.mcp.json`, and a starter `svetra.yaml`. Or add just the server: `claude mcp add svetra -- npx -y svetra mcp`.

**A state change is a policy, not custom code.** A policy's `when` is the row as it is and its `check` is the row after an update, so "a draft may become submitted, a submitted document approved or rejected, and nothing else" is a few lines of `svetra.yaml`. A `restrictive: true` policy must hold on top of the others, so "suspended people can't write" is one rule. The generated tests try every value a rule reads and prove it on your database ([spec](docs/update-check.md)).

**Say which fields a person may change.** `only: [status]` on a reviewer's policy lets them approve or reject a document and change nothing else, so "approve and retitle in one update" is refused. It is an allow-list, so a column added later stays locked. Svetra generates the guard (a trigger, because a policy never sees the old and new row together), and the access tests prove it on your database, including a locked field changed beside an allowed status change. The service role, the owner and custom code are exempt, as they are from row-level security ([spec](docs/column-rules.md)).

**Custom code, fenced in.** Logic that a rule can't express, like moving money between accounts, goes in a *hole*: a Postgres function whose body your agent writes in `supabase/holes/transfer.sql`. Svetra writes everything around it. The function runs with access to only the tables the hole declares, and a `when` condition such as `source.owner == auth.user` refuses anyone else before the body runs. The access tests prove both, and `src/svetra.gen.ts` gets a typed `transfer(client, input)`.

**Sign-in settings, safe by default.** Add `auth: { sign_in: [email, google] }` and `compile` sets Supabase's `supabase/config.toml`: 10-character passwords, confirmed emails, OAuth secrets kept in environment variables, and nothing else in the file touched. Mark a role `mfa: true` and it only counts once the person has verified an authenticator app.

It validates first and writes nothing if there are errors. The output is byte-identical for the same input. Every PR applies the generated SQL to a real PostgreSQL 16 and checks that each constraint and policy behaves as declared: who can see which rows, and what gets rejected. The full output is specified in [`docs/compiler.md`](docs/compiler.md).

**Check an app you already have:** `svetra check --db <connection string>` (or `--local`) finds the leaks in any Supabase database, no `svetra.yaml` needed: tables anyone can read, open policies, views and functions that skip row-level security. Each finding comes in plain English with the fix ([spec](docs/check.md)). No database to point at? `svetra check --migrations` builds your `supabase/migrations` in a throwaway Docker database and checks that, and says which migrations it couldn't read.

**See who can see what:** `svetra view` writes one page, `.svetra/view.html`, and opens it: a table of every person (signed out, signed in, each role) by every thing and action, answered All, Some or No. Select a cell to see which rows the tests tried, the rule behind it, and the test numbers that check it. The answers come from the same evaluator as the access tests, so the page can't disagree with them.

**Editor support:** add this as the first line of `svetra.yaml` for autocomplete in VS Code, Zed or Neovim:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/555aaditya/Svetra/main/schema/svetra.schema.json
```

The full format is specified in [`docs/format.md`](docs/format.md).

## What Svetra is, and isn't

- **It doesn't remove code; it removes boilerplate.** Business logic such as pricing, webhooks and emails still needs code. That code lives only in declared hole files, usually written by your agent. The compiler generates each hole's typed signature, and the hole can reach only the data it declares. Generated files are never hand-edited, and Svetra will report it if they are.
- **There's no AI inside Svetra.** Your agent does the creative work. Svetra checks it and compiles it, deterministically.
- **It's not an app builder.** It sits underneath your agent, or under tools like Lovable and Bolt, and keeps what they build correct, secure and understandable.

## Roadmap

| Step | What | Status |
|---|---|---|
| Format and validator | `svetra.yaml` v0.1, 28 checks, JSON output, JSON Schema | **Done** |
| Compiler | PostgreSQL tables, row-level security, TypeScript types and Zod validators for Supabase and React | **Done** |
| Understand | `svetra explain` (your app in plain English), `svetra graph` (a diagram), `svetra context` (compact context for agents) | **Done** |
| Roles and teams | `roles:` (global like `admin`, or scoped like `staff` at one location) and `has_role()` in rules, compiled to a private grants table and a locked-down check function | **Done** |
| Access tests | Generated pgTAP tests: every person × action × row, as real users, with every failure reported | **Done** |
| Sign-in settings | `auth:` (methods, signup, email confirmation, password rules) compiled into `supabase/config.toml`, and `mfa: true` roles that need a second factor | **Done** |
| Agents | The Svetra skill for Claude Code, `svetra init`, an MCP server for any agent, and holes: custom code as guarded Postgres functions | **Done** |
| See who can see what | `svetra view`: one page with a table of every person × thing × action, in words, and a diagram ([spec](docs/view.md)) | **Done** |
| Automations | Scheduled jobs, triggers when data changes, and incoming webhooks | Planned |
| Start fast | `svetra new` with templates ([spec](docs/new.md)) | **Done** |
| Change safely | `svetra diff` and `svetra plan`: classify every change as safe, breaking or destructive, show whose access widens, and keep renamed data ([spec](docs/change.md)) | **Done** |
| Existing apps | `svetra check` finds the leaks in an app you've already built ([spec](docs/check.md)); `svetra infer` drafts its `svetra.yaml` | `check` **Done**; `infer` planned |
| Pre-launch checklist | `svetra doctor`: `check` plus leaked keys, auth settings, public buckets, backups and stale generated files, in one command ([spec](docs/doctor.md)) | Folder checks (leaked keys, committed `.env`, stale generated files) **Done**; data exposure, auth settings and buckets planned |
| Feature packs | `svetra add teams`, `svetra remove stripe` | Planned |

## Contributing

Svetra is early, and the rules are in [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md). Security issues go through the process in [`SECURITY.md`](SECURITY.md), never a public issue.

## Licence

MIT. See [`LICENSE`](LICENSE).
