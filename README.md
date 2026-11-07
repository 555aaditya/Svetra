# Svetra

**Design the system with your AI. Svetra compiles it to the low level.**

Website: [555aaditya.github.io/Svetra](https://555aaditya.github.io/Svetra/)

People now build apps by talking to AI agents, and most of them can't read the code those agents write. That code decides the things that matter most: what data exists, which rules always hold, and who can see what. When it's wrong, users' data leaks. The most common case is a database table that anyone on the internet can read.

Svetra moves those decisions out of the code and into one small, typed file, `svetra.yaml`. Your agent writes and edits it while you design the app together. Svetra checks every change, and will compile it into the database schema, security policies, types and validators, deterministically, the same way every time. You review a short file and a plain-English explanation instead of thousands of generated lines, and your agent spends fewer tokens because it stops rewriting boilerplate.

> **Status: early.** Version 0.1 of the format, the validator and the compiler work today. See the [roadmap](#roadmap).

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

## What works today: `svetra validate`

**Install** on macOS or Linux; it downloads a release and checks its SHA-256:

```sh
curl -fsSL https://raw.githubusercontent.com/555aaditya/Svetra/main/install.sh | sh
```

Or with Node 18+, on macOS, Linux or Windows. The npm package downloads the same release binary on first run, checks its SHA-256, and caches it:

```sh
npx svetra --version              # run it without installing
npm install -g svetra             # or put `svetra` on your PATH
npm install --save-dev svetra     # or pin it per project, then `npx svetra ...`
```

Or with Rust 1.88+: `cargo install svetra`. Windows builds are also on the [releases page](https://github.com/555aaditya/Svetra/releases).

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

There are 21 checks. They cover tables with no access policy, weak sign-in settings, money stored as `float`, references to things that don't exist, policy and constraint expressions that don't type-check, enum typos, and more. The exit code is `0` for valid, `1` for errors and `2` for a usage error.

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

**Sign-in settings, safe by default.** Add `auth: { sign_in: [email, google] }` and `compile` sets Supabase's `supabase/config.toml`: 10-character passwords, confirmed emails, OAuth secrets kept in environment variables, and nothing else in the file touched. Mark a role `mfa: true` and it only counts once the person has verified an authenticator app.

It validates first and writes nothing if there are errors. The output is byte-identical for the same input. Every PR applies the generated SQL to a real PostgreSQL 16 and checks that each constraint and policy behaves as declared: who can see which rows, and what gets rejected. The full output is specified in [`docs/compiler.md`](docs/compiler.md).

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
| Format and validator | `svetra.yaml` v0.1, 21 checks, JSON output, JSON Schema | **Done** |
| Compiler | PostgreSQL tables, row-level security, TypeScript types and Zod validators for Supabase and React | **Done** |
| Understand | `svetra explain` (your app in plain English), `svetra graph` (a diagram), `svetra context` (compact context for agents) | **Done** |
| Roles and teams | `roles:` (global like `admin`, or scoped like `staff` at one location) and `has_role()` in rules, compiled to a private grants table and a locked-down check function | **Done** |
| Access tests | Generated pgTAP tests: every person × action × row, as real users, with every failure reported | **Done** |
| Sign-in settings | `auth:` (methods, signup, email confirmation, password rules) compiled into `supabase/config.toml`, and `mfa: true` roles that need a second factor | **Done** |
| Agents | The Svetra skill for Claude Code, Cursor and Codex; an MCP server; hole files and recipes | Planned |
| Automations | Scheduled jobs, triggers when data changes, and incoming webhooks | Planned |
| Start fast | `svetra new` with templates | Planned |
| Change safely | `svetra plan` (classifies changes as safe, breaking or destructive), `svetra undo` | Planned |
| Existing apps | `svetra infer` and `svetra check` for apps you've already built | Planned |
| Feature packs | `svetra add teams`, `svetra remove stripe` | Planned |

## Contributing

Svetra is early, and the rules are in [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md). Security issues go through the process in [`SECURITY.md`](SECURITY.md), never a public issue.

## Licence

MIT. See [`LICENSE`](LICENSE).
