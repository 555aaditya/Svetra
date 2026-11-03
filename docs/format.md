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

## Expressions

Constraints and policy conditions are written in a small expression language, inside a YAML string.

```
expr  := or
or    := and ("or" and)*
and   := not ("and" not)*
not   := "not" not | cmp
cmp   := sum (op sum)? | sum "in" "[" literal ("," literal)* "]"
op    := "==" | "!=" | "<" | "<=" | ">" | ">="
sum   := term (("+" | "-") term)*
term  := unary (("*" | "/") unary)*
unary := "-" unary | atom
atom  := number | string | "true" | "false" | "null" | path | "(" expr ")"
path  := name ("." name)?
```

- **Strings** use single quotes: `status == 'active'`.
- **Paths** are a field of the entity (`balance`), a field one reference away (`source.owner`), or `auth.user`, the authenticated user.
- **Types must match.**
  - `int`, `float`, `decimal` and `money` compare and combine with each other.
  - `user` fields compare with `auth.user`.
  - `== null` and `!= null` work on any field.
  - `and`, `or` and `not` need booleans.
  - The whole expression must be boolean.
  - A type error is `SVT003`.
- **Null behaves as in SQL.** A constraint over an empty optional field passes. Write `required: true`, or add `x != null`, when empty must fail. The generated validator does the same, so both sides agree.

Where each path form is allowed:

| | Own fields | `ref.field` (one hop) | `auth.user` |
|---|---|---|---|
| Entity constraint | yes | no: a SQL `check` can't read other tables | no |
| Policy `when` | yes | yes | yes |

Using a form where it isn't allowed is `SVT003`.

## Policies

| Key | Required | Meaning |
|---|---|---|
| `on` | yes | The entity the policy protects (`SVT002` if unknown) |
| `allow` | yes | Non-empty list of `select`, `insert`, `update`, `delete` |
| `when` | yes | Boolean expression a row must satisfy |
| `as` | no | `authenticated` (default) or `anon` |

- **Row-level security is on for every table.** That holds whether or not a policy exists, so access is denied by default.
- **Each action becomes one PostgreSQL policy**, named `<policy>_<action>` when the policy allows more than one action:
  - `select` and `delete` use `when` as the `using` clause.
  - `insert` uses it as the `with check` clause.
  - `update` uses it as both.
- **`auth.user` compiles to `(select auth.uid())`**, the form Supabase recommends for performance.
- **A one-hop path compiles to an `exists` subquery** on the referenced table.
- **A policy on a private entity does nothing**, so it's a warning (`SVT027`).

## Holes

A hole is a place where custom code is allowed, usually written by the user's agent. The compiler generates the signature, types and validation, so only the logic is written by hand.

| Key | Required | Meaning |
|---|---|---|
| `kind` | yes | `function`, the only kind in 0.1 |
| `input` | no | Map of parameter name to type. See below |
| `output` | no | An entity name, an `Entity.field`, or a type. Omit it for no return value |
| `reads` | no | Entities the hole may read |
| `writes` | no | Entities the hole may write |

A parameter or output type is one of three things:
- `Entity.field`, which gives the same type as that field. This is preferred, because the two can't drift apart.
- A bare type name, for types without options: `text`, `int`, `float`, `bool`, `uuid`, `timestamp`, `date`, `json`.
- A full field spec, such as `{ type: money, precision: 18, scale: 6 }`.

Every entity named in `reads`, `writes`, `input` or `output` must exist (`SVT002`). The code a hole runs gets access only to the entities it declares.
