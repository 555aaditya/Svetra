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

## Diagnostics

`svetra validate` checks a file in two stages. Structural checks run first. Graph rules run only on a structurally valid file.

| Code | Severity | Rule | Remediation action |
|---|---|---|---|
| `SVT020` | error | Unknown key | `remove_key` |
| `SVT021` | error | Name breaks its naming pattern | `rename` |
| `SVT022` | error | Entity has no primary field, or more than one | `set_primary` |
| `SVT023` | error | Unknown type, or a missing or invalid type option | `fix_type` |
| `SVT024` | error | Duplicate name | `rename` |
| `SVT025` | error | Unsupported format version | `set_version` |
| `SVT026` | error | `unique` or `indexes` names a field the entity doesn't have | `fix_field` |
| `SVT027` | warning | Policy on a private entity has no effect | `remove_policy` |
| `SVT001` | error | Entity exposed through the API has no policy. Mark it `private: true` or add a policy | `add_policy` |
| `SVT002` | error | Reference to an entity or field that doesn't exist | `fix_reference` |
| `SVT003` | error | Expression has a type error, or uses a path form not allowed where it appears | `fix_expression` |
| `SVT004` | warning | Entity has no write path: no policy allows `insert` or `update`, and no hole lists it in `writes` | `add_write_path` |
| `SVT005` | error | Field whose name suggests money (`amount`, `price`, `balance`, `cost`, `total`, `fee`) uses `float` | `use_money` |
| `SVT010` | info | Reference field is not the leading column of any index or unique constraint. The compiler adds an index | none |

Codes are stable. A code is never reused for a different rule.

With `--json`, each diagnostic has this shape:

```json
{
  "code": "SVT001",
  "category": "policy",
  "severity": "error",
  "path": "entities.Transaction",
  "line": 18,
  "column": 3,
  "message": "Transaction is exposed through the API but has no policy.",
  "retryable": false,
  "remediation": {
    "action": "add_policy",
    "hint": "Add a policy on Transaction, or mark it private: true."
  }
}
```

- **`category`** is one of `syntax`, `naming`, `type`, `reference`, `expression`, `policy`, `money`, `index` and `version`.
- **`retryable` is always `false`** for validation, because repeating the same call with the same file gives the same result. Fix the file instead.
- **Exit codes:**
  - `0`: the file is valid. Warnings and info are allowed.
  - `1`: at least one error.
  - `2`: usage error.

## Full example

```yaml
svetra: "0.1"
system: FinLedger

entities:
  Account:
    fields:
      id:         { type: uuid, primary: true }
      owner:      { type: user, required: true }
      currency:   { type: enum, values: [USD, EUR, GBP, INR], required: true }
      balance:    { type: money, precision: 18, scale: 6, required: true }
      status:     { type: enum, values: [active, frozen, closed], default: active }
      created_at: { type: timestamp, auto: now }
    constraints:
      balance_non_negative: "balance >= 0"
    unique:
      - [owner, currency]

  Transaction:
    fields:
      id:          { type: uuid, primary: true }
      source:      { type: ref, entity: Account, required: true }
      destination: { type: ref, entity: Account, required: true }
      amount:      { type: money, precision: 18, scale: 6, required: true }
      status:      { type: enum, values: [pending, committed, failed, reversed] }
      created_at:  { type: timestamp, auto: now }
    constraints:
      amount_positive: "amount > 0"
      different_accounts: "source != destination"
    indexes:
      - [source, created_at]
      - [destination, created_at]

policies:
  owner_reads_own_accounts:
    on: Account
    allow: [select]
    when: "owner == auth.user"
  owner_reads_own_transactions:
    on: Transaction
    allow: [select]
    when: "source.owner == auth.user or destination.owner == auth.user"

holes:
  transfer:
    kind: function
    input:
      source: Account.id
      destination: Account.id
      amount: Transaction.amount
    output: Transaction
    reads: [Account]
    writes: [Account, Transaction]
```

This file is valid and produces no diagnostics:
- `Account.owner` needs no extra index, because the unique constraint on `[owner, currency]` already leads with it (`SVT010`).
- `Transaction.source` and `Transaction.destination` lead the two declared indexes.
- Both entities have a write path through `transfer` (`SVT004`).

## Not in version 0.1

These come later, if real use asks for them:
- Services, connectors, infrastructure and UI nodes
- Storage-engine selection
- Migrations between graph revisions (planned for `svetra diff` and `patch`)
- Multiple files and imports
- Hole kinds other than `function`, such as webhook recipes
