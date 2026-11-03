# `svetra.yaml` format, version 0.1

`svetra.yaml` describes the data and access layer of an application: entities, constraints, policies, and the holes where custom code goes. Svetra validates it, compiles it, and checks the running system against it. This document is the specification. When the parser and this document disagree, that's a bug in one of them.

Version 0.1 covers one target: Supabase (PostgreSQL with row-level security and the PostgREST API) plus TypeScript types and Zod validators for a React client.

## File

- One file, `svetra.yaml`, at the project root.
- YAML 1.2. Keys are case-sensitive.
- Unknown keys are errors, not warnings (`SVT020`). A typo should never silently do nothing.

## Top level

| Key | Required | Type | Meaning |
|---|---|---|---|
| `svetra` | yes | string | Format version. Must be `"0.1"` (`SVT025` otherwise) |
| `system` | yes | name | The application's name, in PascalCase |
| `entities` | yes | map of entity name to entity | The data model. At least one entity |
| `policies` | no | map of policy name to policy | Who may read or write which rows |
| `holes` | no | map of hole name to hole | Places where custom code is allowed |

## Names

| Thing | Pattern | Example |
|---|---|---|
| System, entity | PascalCase: `^[A-Z][A-Za-z0-9]*$` | `Account`, `LedgerEntry` |
| Field, constraint, policy, hole | snake_case: `^[a-z][a-z0-9_]*$` | `created_at`, `owner_reads_own` |

- A name that breaks its pattern is `SVT021`.
- Names must be unique within their map. Policy, hole and constraint names must also be unique across the whole file, because they become database object names (`SVT024`).
- The table for an entity is its name in snake_case, in the `public` schema: `LedgerEntry` becomes `public.ledger_entry`.

## Entities

| Key | Required | Type | Meaning |
|---|---|---|---|
| `fields` | yes | map of field name to field | At least one field |
| `constraints` | no | map of constraint name to expression | Row-level checks. Each becomes a named SQL `check` and a validator refinement |
| `unique` | no | list of field lists | Each list becomes a named unique constraint |
| `indexes` | no | list of field lists | Each list becomes an index, in the given column order |
| `private` | no | bool, default `false` | A private entity is not exposed through the generated API |

Each entity has exactly one field with `primary: true` (`SVT022`).

## Fields

| Key | Applies to | Meaning |
|---|---|---|
| `type` | all | Required. One of the types below (`SVT023` if unknown) |
| `required` | all | `true` makes the column `not null`. Default `false` |
| `primary` | all | Marks the primary key. Implies `required` |
| `unique` | all | Shorthand for a single-column unique constraint |
| `default` | all except `ref`, `user` | A literal default value, checked against the type |
| `auto` | `timestamp`, `date` | `now`: filled in by the database on insert |
| `values` | `enum` | Required for `enum`. Non-empty list of distinct strings |
| `precision`, `scale` | `decimal`, `money` | Required. `0 < precision <= 38`, `0 <= scale <= precision` |
| `entity` | `ref` | Required for `ref`. The referenced entity |

## Types

| Type | PostgreSQL | TypeScript | Zod | Notes |
|---|---|---|---|---|
| `text` | `text` | `string` | `z.string()` | |
| `int` | `bigint` | `number` | `z.number().int()` | 64-bit in the database. Values above 2^53 lose precision in JavaScript |
| `float` | `double precision` | `number` | `z.number()` | Never for money (`SVT005`) |
| `bool` | `boolean` | `boolean` | `z.boolean()` | |
| `uuid` | `uuid` | `string` | `z.string().uuid()` | A primary `uuid` gets `default gen_random_uuid()` |
| `timestamp` | `timestamptz` | `string` | `z.string().datetime()` | Always stored with a time zone |
| `date` | `date` | `string` | `z.string().date()` | |
| `decimal` | `numeric(p, s)` | `string` | decimal string validator | Exact. Sent as a string so no precision is lost |
| `money` | `numeric(p, s)` | `string` | decimal string validator | Same as `decimal`, but marks the field as money for rule checks |
| `enum` | `text` with a `check` on the values | union of string literals | `z.enum([...])` | |
| `json` | `jsonb` | `unknown` | `z.unknown()` | Escape hatch. Not checked |
| `ref` | `uuid` with `references` | `string` | `z.string().uuid()` | Foreign key to the referenced entity's primary field, which must be a `uuid` |
| `user` | `uuid references auth.users (id)` | `string` | `z.string().uuid()` | The authenticated user who owns or touches the row |
