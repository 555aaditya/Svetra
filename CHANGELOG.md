# Changelog

## 0.1.0 (unreleased)

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
