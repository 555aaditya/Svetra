# `svetra.yaml` format, version 0.1

`svetra.yaml` describes the data and access layer of an application: entities, constraints, policies, and the holes where custom code goes. Svetra validates it, compiles it, and checks the running system against it. This document is the specification. When the parser and this document disagree, that's a bug in one of them.

Version 0.1 covers one target: Supabase (PostgreSQL with row-level security and the PostgREST API) plus TypeScript types and Zod validators for a React client.

## File

- One file, `svetra.yaml`, at the project root.
- YAML 1.2. Keys are case-sensitive.
- Unknown keys are errors, not warnings (`SVT020`). A typo should never silently do nothing.
- **Editor support:** `schema/svetra.schema.json` gives autocomplete and inline checks in editors that use the YAML language server (VS Code, Zed, Neovim). Add this as the first line of `svetra.yaml`:

  ```yaml
  # yaml-language-server: $schema=https://raw.githubusercontent.com/555aaditya/Svetra/main/schema/svetra.schema.json
  ```

  The schema covers keys, types and names. `svetra validate` remains the authority and checks much more: references, policies, expressions and money.

## Top level

| Key | Required | Type | Meaning |
|---|---|---|---|
| `svetra` | yes | string | Format version. Must be `"0.1"` (`SVT025` otherwise) |
| `system` | yes | name | The application's name, in PascalCase |
| `entities` | yes | map of entity name to entity | The data model. At least one entity |
| `policies` | no | map of policy name to policy | Who may read or write which rows |
| `holes` | no | map of hole name to hole | Places where custom code is allowed |
| `roles` | no | map of role name to role | Roles people can hold, such as `admin`, or `staff` at one location |

## Names

| Thing | Pattern | Example |
|---|---|---|
| System, entity | PascalCase: `^[A-Z][A-Za-z0-9]*$` | `Account`, `LedgerEntry` |
| Field, constraint, policy, hole, role | snake_case: `^[a-z][a-z0-9_]*$` | `created_at`, `owner_reads_own`, `admin` |

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
atom  := number | string | "true" | "false" | "null" | call | path | "(" expr ")"
call  := "has_role" "(" string ("," path)? ")"
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

| | Own fields | `ref.field` (one hop) | `auth.user` | `has_role(...)` |
|---|---|---|---|---|
| Entity constraint | yes | no: a SQL `check` can't read other tables | no | no |
| Policy `when` | yes | yes | yes | yes |

Using a form where it isn't allowed is `SVT003`.

## Policies

| Key | Required | Meaning |
|---|---|---|
| `on` | yes | The entity the policy protects (`SVT002` if unknown) |
| `allow` | yes | Non-empty list of `select`, `insert`, `update`, `delete` |
| `when` | yes | Boolean expression a row must satisfy |
| `as` | no | `authenticated` (default): signed-in people. `anyone`: everyone, signed in or not. `anon`: only people who aren't signed in, which is rarely right, because signed-in people then get less (`SVT007`) |

- **Row-level security is on for every table.** That holds whether or not a policy exists, so access is denied by default.
- **Each action becomes one PostgreSQL policy**, named `<policy>_<action>` when the policy allows more than one action:
  - `select` and `delete` use `when` as the `using` clause.
  - `insert` uses it as the `with check` clause.
  - `update` uses it as both.
- **`auth.user` compiles to `(select auth.uid())`**, the form Supabase recommends for performance.
- **A one-hop path compiles to a scalar subquery** on the referenced row's primary key (see `compiler.md`).
- **A policy on a private entity does nothing**, so it's a warning (`SVT027`).

## Roles

Roles say who can do more than an ordinary signed-in person. A role is either **global**, such as `admin`, or **scoped** to rows of one entity: `staff` at a particular `Location`, or `member` of a particular `Org`. Scoped roles are how Svetra expresses teams and multi-tenancy.

| Key | Required | Meaning |
|---|---|---|
| `description` | no | Plain words, used by `svetra explain` |
| `scope` | no | An entity. If set, the role is held *for one row* of that entity. Its primary key must be a `uuid` (`SVT006`) |

### `has_role` in policies

| Call | True when the signed-in person... |
|---|---|
| `has_role('admin')` | holds the global role `admin` |
| `has_role('staff', location)` | holds `staff` scoped to the row that this row's `location` field points to |
| `has_role('member', id)` | holds `member` scoped to *this* row. Used in policies on the scope entity itself, such as `Org` |

Rules, all reported as `SVT006`:
- The role must be declared under `roles`.
- A global role takes no second argument.
- A scoped role needs one.
- The second argument must be either a `ref` to the role's scope entity, or `id` when the policy is on the scope entity itself.

`has_role` is allowed only in policy conditions, never in constraints (`SVT003`).

### Example: bookings with customers, staff and admins

```yaml
roles:
  admin: { description: "Runs the business" }
  staff: { description: "Works at one location", scope: Location }

entities:
  Location:
    fields:
      id:   { type: uuid, primary: true }
      name: { type: text, required: true }
  Booking:
    fields:
      id:       { type: uuid, primary: true }
      customer: { type: user, required: true }
      location: { type: ref, entity: Location, required: true }
      starts:   { type: timestamp, required: true }

policies:
  customers_see_own:  { on: Booking, allow: [select, insert], when: "customer == auth.user" }
  staff_manage_local: { on: Booking, allow: [select, update], when: "has_role('staff', location)" }
  admins_manage_all:  { on: Booking, allow: [select, update, delete], when: "has_role('admin')" }
  staff_see_location: { on: Location, allow: [select], when: "has_role('staff', id)" }
  admins_see_locations: { on: Location, allow: [select, insert, update], when: "has_role('admin')" }
```

**Granting roles** happens in the database. Run, as the service role or in the Supabase SQL editor:

```sql
insert into svetra.role_grants (user_id, role, scope_id) values ('<user id>', 'staff', '<location id>');
```

A generated, admin-only way to grant roles from the app comes with the agent tools (M4). Until then, granting is deliberately not possible from the client.

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
| `SVT019` | error | The file is not valid YAML | `fix_syntax` |
| `SVT020` | error | Unknown key | `remove_key` |
| `SVT021` | error | Name breaks its naming pattern | `rename` |
| `SVT022` | error | Entity has no primary field, or more than one | `set_primary` |
| `SVT023` | error | Unknown type, or a missing or invalid type option | `fix_type` |
| `SVT024` | error | Duplicate name | `rename` |
| `SVT025` | error | Unsupported format version | `set_version` |
| `SVT026` | error | `unique` or `indexes` names a field the entity doesn't have | `fix_field` |
| `SVT027` | warning | Policy on a private entity has no effect | `remove_policy` |
| `SVT028` | error | A required key is missing | `add_key` |
| `SVT029` | error | A value has the wrong kind, such as a string where a list or bool is expected | `fix_value` |
| `SVT001` | error | Entity exposed through the API has no policy. Mark it `private: true` or add a policy | `add_policy` |
| `SVT002` | error | Reference to an entity or field that doesn't exist | `fix_reference` |
| `SVT003` | error | Expression has a type error, or uses a path form not allowed where it appears | `fix_expression` |
| `SVT004` | warning | Entity has no write path: no policy allows `insert` or `update`, and no hole lists it in `writes` | `add_write_path` |
| `SVT005` | error | Field whose name suggests money (`amount`, `price`, `balance`, `cost`, `total`, `fee`) uses `float` | `use_money` |
| `SVT006` | error | `has_role` names an undeclared role, has the wrong number of arguments, or its scope argument doesn't point at the role's scope entity; or a role's `scope` entity doesn't have a `uuid` primary key | `fix_role` |
| `SVT007` | warning | A policy with `as: anon` gives people who aren't signed in access that signed-in people may not get. No policy for signed-in people covers the same action on the same entity with the same condition or with `true`, so signing in could take access away. Usually you meant `as: anyone` | `use_anyone` |
| `SVT010` | info | A `ref` or `user` field is not the leading column of any index or unique constraint. The compiler adds an index, because these are the columns joins and policies filter on | none |

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

- **The JSON output** wraps the diagnostics with the file name and an overall verdict: `{"file": "svetra.yaml", "valid": false, "diagnostics": [ ... ]}`. Diagnostics are sorted by position.
- **`line` and `column`** are both 1-based, as in editors.
- **`category`** is one of `syntax`, `naming`, `type`, `reference`, `expression`, `policy`, `role`, `money`, `index` and `version`.
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
