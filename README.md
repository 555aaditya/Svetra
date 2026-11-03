# Svetra

**Design the system with your AI. Svetra compiles it to the low level.**

People now build apps by talking to AI agents, and most of them can't read the code those agents write. That code decides the things that matter most: what data exists, which rules always hold, and who can see what. When it's wrong, users' data leaks. The most common case is a database table that anyone on the internet can read.

Svetra moves those decisions out of the code and into one small, typed file, `svetra.yaml`. Your agent writes and edits it while you design the app together. Svetra checks every change, and will compile it into the database schema, security policies, types and validators, deterministically, the same way every time. You review a short file and a plain-English explanation instead of thousands of generated lines, and your agent spends fewer tokens because it stops rewriting boilerplate.

> **Status: early.** Version 0.1 of the format and the validator work today. The compiler is next. See the [roadmap](#roadmap).

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

```sh
cargo install --git https://github.com/555aaditya/Svetra
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

There are 17 checks. They cover tables with no access policy, money stored as `float`, references to things that don't exist, policy and constraint expressions that don't type-check, enum typos, and more. The exit code is `0` for valid, `1` for errors and `2` for a usage error.

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
| Format and validator | `svetra.yaml` v0.1, 17 checks, JSON output, JSON Schema | **Done** |
| Compiler | PostgreSQL tables, row-level security, TypeScript types and Zod validators for Supabase and React | Next |
| Understand | `svetra explain` (your app in plain English), `svetra graph` (a diagram), `svetra context` (compact context for agents) | Planned |
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
