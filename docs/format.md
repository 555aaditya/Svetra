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
| `audit` | no | bool, default `false` | Every insert, update and delete of a row is recorded in the audit log, with who did it ([Audit](#audit)) |
| `was` | no | name | What this entity was called in the design the database has: declares a rename, so `svetra plan` keeps its rows ([change.md](change.md)) |

Each entity has exactly one field with `primary: true` (`SVT022`).

## Fields

| Key | Applies to | Meaning |
|---|---|---|
| `type` | all | Required. One of the types below (`SVT023` if unknown) |
| `required` | all | `true` makes the column `not null`. Default `false` |
| `primary` | all | Marks the primary key. Implies `required`. A `user` primary key (a profile keyed by the auth user's id) gives each person at most one row |
| `unique` | all | Shorthand for a single-column unique constraint |
| `default` | all except `ref`, `user` | A literal default value, checked against the type |
| `auto` | `timestamp`, `date` | `now`: filled in by the database on insert |
| `values` | `enum` | Required for `enum`. Non-empty list of distinct strings |
| `precision`, `scale` | `decimal`, `money` | Required. `0 < precision <= 38`, `0 <= scale <= precision` |
| `entity` | `ref` | Required for `ref`. The referenced entity |
| `on_delete` | `ref` | What deleting the referenced row does to this one: `restrict` (default), `cascade` or `set_null`. See [Deleting a referenced row](#deleting-a-referenced-row) |
| `was` | all | What this field was called in the design the database has: declares a rename, so `svetra plan` keeps its values ([change.md](change.md)) |

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
| `ref` | `uuid` with `references` | `string` | `z.string().uuid()` | Foreign key to the referenced entity's primary field, which must be a `uuid`. `on_delete` says what deleting the referenced row does |
| `user` | `uuid references auth.users (id)` | `string` | `z.string().uuid()` | The authenticated user who owns or touches the row |

### Deleting a referenced row

A `ref` is a foreign key, so deleting a row that other rows still point at has to do something. `on_delete`, written on the `ref` field, says what:

| `on_delete` | Deleting the referenced row... | PostgreSQL |
|---|---|---|
| `restrict` (default) | is refused while any row points at it (SQLSTATE `23503`) | `no action` |
| `cascade` | deletes every row that points at it, and the rows that point at those | `on delete cascade` |
| `set_null` | clears the field on every row that points at it. The field must not be `required` (`SVT023`) | `on delete set null` |

```yaml
Message:
  fields:
    channel: { type: ref, entity: Channel, required: true, on_delete: cascade }
```

- **Only `ref` fields take it** (`SVT023` on any other type). A `user` field's foreign key to `auth.users` stays `restrict`: Svetra never deletes people.
- **The database does the cascade, not the person.** A cascade or a `set_null` runs as the table's owner, so it ignores the policies of the rows it deletes or changes: a person who may delete a `Channel` also deletes its `Message`s, whether or not a delete policy on `Message` would have let them. Row-level security applies only to the row named in the `delete`.
- **A refusal is not an access rule.** `restrict` refuses after the policies have allowed the delete, so a person who may not delete the row is still told "no rows", and one who may is told `23503` while a row still points at it. The generated access tests expect each outcome from this option and the policies ([compiler.md](compiler.md#access-tests)).
- **A row that points at itself** doesn't block its own deletion.

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
| Trigger hole `when` | yes, the row after the change | no | no | no |

A trigger hole's `when` also has `old.<field>`, the row before the change, when its `events` is `[update]` ([Triggered holes](#triggered-holes)). Using a form where it isn't allowed is `SVT003`.

## Policies

| Key | Required | Meaning |
|---|---|---|
| `on` | yes | The entity the policy protects (`SVT002` if unknown) |
| `allow` | yes | Non-empty list of `select`, `insert`, `update`, `delete` |
| `when` | yes | Boolean expression a row must satisfy. For an update it is the condition on the row **before**; see `check` |
| `check` | no | The condition the row must satisfy **after** an update (`WITH CHECK`). When absent it is `when`. Only for a policy that allows `update` (`SVT033` otherwise); typed like `when` |
| `only` | no | The fields an update allowed through this policy may change; every other field must stay as it is, so a column added later stays locked. Without it the policy frees every field. At least one field of the entity, never its primary key (`SVT002`); only for a policy that allows `update` (`SVT035`). See [column-rules.md](column-rules.md) |
| `restrictive` | no | `true`: the policy must hold **in addition to** at least one permissive policy that allows the action, and never grants access by itself. Default `false` |
| `as` | no | `authenticated` (default): signed-in people. `anyone`: everyone, signed in or not. `anon`: only people who aren't signed in, which is rarely right, because signed-in people then get less (`SVT007`) |

- **Row-level security is on for every table.** That holds whether or not a policy exists, so access is denied by default.
- **Each action becomes one PostgreSQL policy**, named `<policy>_<action>` when the policy allows more than one action:
  - `select` and `delete` use `when` as the `using` clause.
  - `insert` uses it as the `with check` clause.
  - `update` uses `when` as the `using` clause and `check` (or `when`, when there is no `check`) as the `with check` clause.
- **`check` lets a rule say what an update may turn a row into.** `when: "status == 'draft'"` with `check: "status in ['draft', 'submitted']"` lets a draft stay a draft or become submitted, and nothing else. Without a `check`, the row must satisfy `when` both before and after, which can't express a state change.
- **Permissive policies combine with `or`, separately for the old row and the new row,** exactly as PostgreSQL does. An update is allowed when the old row passes *some* policy's `when` and the new row passes *some* policy's `check`, which need not be the same policy: with `using (x = 1) with check (x = 1)` in one policy and `using (x = 2) with check (x = 3)` in another, a row with `x = 1` **can** be updated to `x = 3` (checked on PostgreSQL 16). `svetra validate` warns when that happens by accident (`SVT034`).
- **A restrictive policy only narrows.** All restrictive policies for an action must hold, each on its own `when` (and `check` for updates), on top of one permissive policy. It compiles to `create policy ... as restrictive`. An entity with only restrictive policies lets nobody in, which is `SVT001`.
- **`auth.user` compiles to `(select auth.uid())`**, the form Supabase recommends for performance.
- **A one-hop path compiles to a scalar subquery** on the referenced row's primary key (see `compiler.md`).
- **A policy on a private entity does nothing**, so it's a warning (`SVT027`).

## Roles

Roles say who can do more than an ordinary signed-in person. A role is either **global**, such as `admin`, or **scoped** to rows of one entity: `staff` at a particular `Location`, or `member` of a particular `Org`. Scoped roles are how Svetra expresses teams and multi-tenancy.

| Key | Required | Meaning |
|---|---|---|
| `description` | no | Plain words, used by `svetra explain` |
| `scope` | no | An entity. If set, the role is held *for one row* of that entity. Its primary key must be a `uuid` (`SVT006`) |
| `mfa` | no | `true`: the role only counts in a session verified with a second factor, an authenticator app. Default `false`. See [Authentication](#authentication) |

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

**From the app,** roles are given and taken away by a hole that lists them in `grants` (see [Holes](#holes)), such as one that starts a team and makes its creator a member, or one that lets a member add someone else. The client can never write grants directly, and a hole can only reach the roles it lists.

```yaml
holes:
  start_team:
    kind: function
    input: { name: Team.name }
    output: Team
    writes: [Team]
    grants: [member]
```

## Authentication

The optional `auth` section says how people sign in. Without it, Supabase's own defaults apply: email sign-in with passwords of 6 characters, and no email confirmation. `svetra validate` points that out as `SVT009` (info).

```yaml
auth:
  sign_in: [email, google]
  signup: open
  confirm_email: true
  password: { min_length: 12, require: lower_upper_letters_digits }

roles:
  admin: { description: "Runs the business", mfa: true }
```

| Key | Required | Meaning |
|---|---|---|
| `sign_in` | yes | Non-empty list of methods. `email` is a password or an emailed sign-in link, through Supabase's email provider. The others are OAuth providers: `apple`, `bitbucket`, `discord`, `facebook`, `github`, `gitlab`, `google`, `linkedin_oidc`, `notion`, `slack_oidc`, `spotify`, `twitch`, `twitter`, `x`, `zoom` |
| `signup` | no | `open` (default): anyone can create an account. `invite_only`: only people you invite, and existing users can still sign in |
| `confirm_email` | no | `true` (default): people confirm their email address before they can sign in |
| `password.min_length` | no | At least 8. Default 10 |
| `password.require` | no | `none` (default), `letters_digits`, `lower_upper_letters_digits` or `lower_upper_letters_digits_symbols` |

Rules, all reported as `SVT008`:
- Every `sign_in` method must be in the list above.
- `password` and `confirm_email` need `email` in `sign_in`.
- `password.min_length` must be at least 8.

**Second factors for roles.** A role with `mfa: true` only counts when the person's current session was verified with an authenticator app. Signed in with a password alone, an `admin` gets what any signed-in person gets, and no more. Use it for roles that can see or change everyone's data.

**Not in version 0.1:** phone and SMS sign-in; anonymous sign-ins, which Svetra keeps off, because Supabase gives anonymous users the `authenticated` role, so every `as: authenticated` rule would apply to them; SAML and enterprise single sign-on; the providers that need extra settings (`azure`, `keycloak`, `workos`); and per-environment site URLs, planned with environments.

## Holes

A hole is a place where custom code is allowed, usually written by the user's agent. The compiler generates the signature, types and validation, so only the logic is written by hand.

The keys below are those of `kind: function`. Each kind takes its own keys, and a key the kind doesn't take is `SVT020`, with a hint that lists the keys it does. The design for the other kinds, which run with no caller, is in [automations.md](automations.md).

| Key | Required | Meaning |
|---|---|---|
| `kind` | yes | `function` (called by a person), `schedule` (run by the database's clock, see [Scheduled holes](#scheduled-holes)) or `trigger` (run when a row changes, see [Triggered holes](#triggered-holes)) |
| `input` | no | Map of parameter name to type. See below |
| `output` | no | An entity name, an `Entity.field`, or a type. Omit it for no return value |
| `reads` | no | Entities the hole may read |
| `writes` | no | Entities the hole may write |
| `grants` | no | Roles the hole may give people and take away, such as `[member]`. No other role is reachable from it |
| `as` | no | Who may call it: `authenticated` (default), `anyone` or `anon`, as for policies |
| `when` | no | A condition on the inputs that must hold for the call to run, such as `source.owner == auth.user`. Default `true` |
| `require` | no | List of conditions on the inputs, each of which must hold, such as `["amount > 0"]`. A call that breaks one is refused as bad input ([Requirements](#requirements)) |
| `audit` | no | bool, default `false`. Every call that runs is recorded in the audit log, with who made it and the inputs ([Audit](#audit)) |

A parameter or output type is one of three things:
- `Entity.field`, which gives the same type as that field. This is preferred, because the two can't drift apart.
- A bare type name, for types without options: `text`, `int`, `float`, `bool`, `uuid`, `timestamp`, `date`, `json`.
- A full field spec, such as `{ type: money, precision: 18, scale: 6 }`.

Every entity named in `reads`, `writes`, `input` or `output` must exist (`SVT002`). Every role named in `grants` must be declared under `roles` (`SVT006`). The code a hole runs gets access only to the entities it declares, and to grants of the roles it lists.

**Giving a role** in a body is an insert into `"svetra"."role_grants"` (`"user_id"`, `"role"`, `"scope_id"`), with the scope row's id for a scoped role and `null` for a global one; taking it away is a delete. For example, `insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values (p_caller, 'member', v_team."id");`. Who may call the hole is still decided by `as` and `when`: a hole that adds members usually has `when: "has_role('member', team)"`.

**The body** is the one hand-written part: PL/pgSQL statements in `supabase/holes/<name>.sql`, next to `svetra.yaml`. **Statements only:** Svetra writes the function around them, including its own `begin` and `end`, so every statement ends with `;`. A whole function body (`begin … end` with no `;` after it) leaves a stray `end` and a migration that doesn't parse, so `compile` refuses it (`SVT032`).
- **Inputs** are named `p_<input>`, such as `p_source`, so they never clash with column names. **`p_caller`** is the signed-in person's id, or `null` for a logged-out visitor.
- **Variables** need their own block inside the body: `declare v_tx "public"."transaction"; begin … end;`.
- **The file** is created with a stub the first time, and never overwritten.

**`when`** uses the expression language of policies, over the hole's inputs instead of a row's fields:
- `p_` isn't written in expressions. `source` means the `source` input.
- An input typed `Entity.<primary key>`, such as `source: Account.id`, also allows one hop to that row's fields: `source.owner` is the owner of the account whose id is `source`.
- `auth.user` and `has_role(...)` work as in policies.
- Errors are `SVT003`, as for policies. That includes reading through an input into an entity the hole doesn't declare in `reads` or `writes`, because the guard runs with the hole's access.

```yaml
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
    when: "source.owner == auth.user"
```

### Requirements

`when` says **who** may call a hole and refuses with SQLSTATE `42501`, which every client reads as "not allowed". `require` says what the **inputs** must be, and refuses with `22023` (`invalid_parameter_value`), so a client can tell "that amount is wrong" from "you may not".

```yaml
holes:
  transfer:
    when: "source.owner == auth.user"
    require:
      - "amount > 0"
      - "source != destination"
```

- **Each item is an expression** in the language of `when`: inputs are written without `p_`, an `Entity.<primary key>` input allows one hop, and `auth.user` and `has_role(...)` work. It must be a boolean (`SVT003` otherwise), and reading through an input into an entity the hole doesn't declare in `reads` or `writes` is `SVT003`, as for `when`. `require` must list at least one condition (`SVT029`).
- **The order is `when`, then `require`, then the body.** Who may call is settled first, so a caller who isn't allowed learns nothing about the rules for input.
- **A null fails.** SQL's `check` lets a null pass; a requirement doesn't, so `amount > 0` refuses a missing amount.
- **The first condition that fails is reported,** in list order, as `<hole>: <the condition as written>`, for example `transfer: amount > 0`.
- **The TypeScript client is unchanged.** The generated Zod schema doesn't repeat a requirement; the database is the authority.

### Scheduled holes

A hole with `kind: schedule` runs on the database's clock, not when a person calls it. Nobody can call it from the app: it has no function in `public` and nothing in `src/svetra.gen.ts`.

```yaml
holes:
  expire_invites:
    kind: schedule
    schedule: "0 3 * * *"        # every day at 03:00 UTC
    reads: [Invite]
    writes: [Invite]
```

| Key | Required | Meaning |
|---|---|---|
| `kind` | yes | `schedule` |
| `schedule` | yes | Five-field cron: minute, hour, day of month, month, day of week, in UTC. Each field is `*`, a number, a range (`1-5`), a list (`1,15`) or a step (`*/15`, `10-30/5`). Day of week is `0` to `7` (`0` and `7` are Sunday). Names such as `mon` are not allowed. A bad expression is `SVT037` |
| `reads`, `writes`, `grants` | no | As for `function` |

Not allowed: `input`, `output`, `require`, `as` and `when`, because nobody passes or receives them, and `audit`, because an audited hole logs a call by a person and a schedule has none (`SVT020`). Write a guard in the body. The body is written as for any hole, and `p_caller` is always `null`. It runs as the hole's own role, with the access `reads`, `writes` and `grants` declare. A schedule with `grants` gives or takes away roles with no person behind it, so it gets `SVT040`. A body should be safe to run twice and to run late: a failed run is not retried and a run missed while the database was down is not made up. The generated SQL, which needs pg_cron, is in [compiler.md](compiler.md#holes).

### Triggered holes

A hole with `kind: trigger` runs after a row of one entity is inserted, updated or deleted, in the same transaction as the change. Nobody can call it from the app: it has no function in `public` and nothing in `src/svetra.gen.ts`.

```yaml
holes:
  notify_paid:
    kind: trigger
    on: Order
    events: [update]
    when: "status == 'paid' and old.status != 'paid'"
    reads: [Order, Customer]
    writes: [Notification]
```

| Key | Required | Meaning |
|---|---|---|
| `kind` | yes | `trigger` |
| `on` | yes | The entity whose rows fire it. Must exist (`SVT002`) |
| `events` | yes | A non-empty list of `insert`, `update` and `delete` (`SVT029` otherwise) |
| `when` | no | A condition on the row that decides whether the body runs. Default: always |
| `reads`, `writes`, `grants` | no | As for `function` |

Not allowed: `input`, `output`, `require` (the row is the input, and `when` already decides whether the body runs), `as` (a row change starts it) and `audit` (an audited hole logs a call by a person, and nobody calls a trigger), all `SVT020`.

- **The row in the body.** `p_new` is the row after the change (`null` on delete) and `p_old` the row before (`null` on insert), both typed as the entity's row. `p_caller` is the person whose change fired it, from the session's `auth.uid()`, and `null` for the service role, a job or a migration. It is information, never a permission check.
- **`when`** becomes the trigger's own `WHEN (...)` clause. Fields are written bare and mean the row after the change (the deleted row for `delete`). `old.<field>` means the row before, and is allowed only when `events` is exactly `[update]`. No other entity, no `auth.user` and no `has_role`: a `WHEN` clause can't run a subquery. These are `SVT003`, as is a non-boolean result.
- **Timing** is always after the row is written, one call per row. A change that raises in the body is rolled back with the hole's work.
- **It runs as the hole's own role,** not the person who made the change. It touches only what `reads`, `writes` and `grants` declare.
- **Loops.** Trigger holes that write each other's entity, or their own, loop: that is `SVT038`, which names the cycle. At run time a trigger nested more than three deep raises `54001`.
- **`grants`** on a trigger gives or takes away roles with no person asked, so it gets `SVT040`.

The generated SQL is in [compiler.md](compiler.md#triggered-holes), and the design in [automations.md](automations.md#triggered-holes).

## Audit

`audit: true` on an entity or a hole records what happened in one append-only table, `svetra.audit_log`. It answers "who changed this, and when" from the database itself, so it holds for every client, not only Svetra's.

| Column | Meaning |
|---|---|
| `id` | Increasing number: the order of events |
| `at` | When it happened (the transaction's time) |
| `actor` | `auth.uid()`: who made the request. `null` for a logged-out visitor, and for work that doesn't come through the API, such as a migration or the service role |
| `kind` | `row` for a change to an audited entity, `call` for a call of an audited hole |
| `subject` | The table name (`ledger_entry`) or the hole name (`transfer`) |
| `action` | `insert`, `update` or `delete` for a row, `call` for a hole |
| `old`, `new` | The row before and after as `jsonb` (`old` is null on insert, `new` on delete). For a call, `new` holds the inputs by name and `old` is null |

- **An audited entity** logs every change, whoever makes it: the API, a hole body, or SQL run by hand. The whole row is logged, so don't audit an entity that holds secrets.
- **An audited hole** logs each call that runs. A refused call raises, which rolls back the whole transaction, so refusals can't be logged in the database: only a call the `when` guard allows stays recorded. If the body then fails, the call's record is rolled back with it. Writes the body makes to audited entities are logged as rows, with the caller as `actor`.
- **Nobody reaches the log from the app.** It has row-level security with no policy and no grants for `anon` or `authenticated`, and the `svetra` schema isn't exposed. Hole code gets no access to it either: the hole's role has no grant, and the record is written by the generated wrapper, not the body.
- **Append-only.** A trigger refuses `update`, `delete` and `truncate` on the table for every role, including its owner. It is tamper-evident, not tamper-proof: a superuser can drop the trigger. Retention, export and pruning are not part of 0.1.
- The log table exists only when something in the file has `audit: true`. Turning audit off stops new records and leaves the existing log in place.

## Diagnostics

`svetra validate` checks a file in two stages. Structural checks run first. Graph rules run only on a structurally valid file.

| Code | Severity | Rule | Remediation action |
|---|---|---|---|
| `SVT019` | error | The file is not valid YAML | `fix_syntax` |
| `SVT020` | error | Unknown key, or a key the hole's `kind` doesn't take | `remove_key` |
| `SVT021` | error | Name breaks its naming pattern | `rename` |
| `SVT022` | error | Entity has no primary field, or more than one | `set_primary` |
| `SVT023` | error | Unknown type, or a missing or invalid type option, including `on_delete` on a field that isn't a `ref`, an unknown value, or `set_null` on a required field | `fix_type` |
| `SVT024` | error | Duplicate name | `rename` |
| `SVT025` | error | Unsupported format version | `set_version` |
| `SVT026` | error | `unique` or `indexes` names a field the entity doesn't have | `fix_field` |
| `SVT027` | warning | Policy on a private entity has no effect | `remove_policy` |
| `SVT028` | error | A required key is missing | `add_key` |
| `SVT029` | error | A value has the wrong kind, such as a string where a list or bool is expected | `fix_value` |
| `SVT030` | error | A rename can't be right: `was` names an entity or field that is still in this design, or (in `svetra plan`) nothing in the design the database has | `fix_rename` |
| `SVT032` | error | In `svetra compile`: a hole body that doesn't end with `;`, typically a whole `begin … end` function body. The generated function wouldn't parse. Reported against `supabase/holes/<name>.sql`, and nothing is written | `fix_hole_body` |
| `SVT033` | error | A policy has a `check` but doesn't allow `update`: only an update has a row after it | `remove_check` |
| `SVT034` | warning | Two permissive update policies combine into a transition neither allows alone: the old row passes one policy's `when` and the new row passes the other's `check`, so the update is allowed although no single policy allows it | `merge_policies` |
| `SVT035` | error | A policy has `only` but doesn't allow `update`: only an update changes fields | `remove_only` |
| `SVT036` | warning | A person is covered, on some row, by two permissive update policies that disagree about a field: one has an `only` that leaves it out and another frees it, so the first one's limit has no effect for that person | `free_or_limit` |
| `SVT031` | info | In `svetra plan`: `was` names a rename that's already applied, because the design the database has already uses the new name. Remove it | `remove_was` |
| `SVT037` | error | A `schedule` isn't a valid five-field cron expression: wrong number of fields, a value out of range, or a name where a number is needed. The message says which field | `fix_schedule` |
| `SVT038` | error | Trigger holes form a loop: one writes an entity whose trigger hole (or its own) fires on it. The message names the cycle | `break_loop` |
| `SVT040` | warning | A `schedule` or `trigger` hole has `grants`: a role is given or taken away with no person calling it | `confirm_grants` |
| `SVT001` | error | Entity exposed through the API has no policy. Mark it `private: true` or add a policy | `add_policy` |
| `SVT002` | error | Reference to an entity or field that doesn't exist | `fix_reference` |
| `SVT003` | error | Expression has a type error, or uses a path form not allowed where it appears, such as `old.` outside an update trigger | `fix_expression` |
| `SVT004` | warning | Entity has no write path: no policy allows `insert` or `update`, and no hole lists it in `writes` | `add_write_path` |
| `SVT005` | error | Field whose name suggests money (`amount`, `price`, `balance`, `cost`, `total`, `fee`) uses `float` | `use_money` |
| `SVT006` | error | `has_role` names an undeclared role, has the wrong number of arguments, or its scope argument doesn't point at the role's scope entity; or a role's `scope` entity doesn't have a `uuid` primary key; or a hole's `grants` names an undeclared role | `fix_role` |
| `SVT007` | warning | A policy with `as: anon` gives people who aren't signed in access that signed-in people may not get. No policy for signed-in people covers the same action on the same entity with the same condition or with `true`, so signing in could take access away. Usually you meant `as: anyone` | `use_anyone` |
| `SVT008` | error | Invalid sign-in settings: an unknown method or value, a method listed twice, a password shorter than 8, or password or `confirm_email` settings without `email` sign-in | `fix_auth` |
| `SVT009` | info | No `auth` section, so Supabase's defaults apply: passwords of 6 characters and no email confirmation | `add_auth` |
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
- **`category`** is one of `syntax`, `naming`, `type`, `reference`, `expression`, `policy`, `role`, `money`, `index`, `hole` and `version`.
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
- Audit retention and export, logging refused calls, and alerting
- Hole kinds other than `function`, `schedule` and `trigger`: webhooks are specified in [automations.md](automations.md)
- Retry keys (`idempotent`) on holes: proposed in [hole-middleware.md](hole-middleware.md)
- The sign-in methods listed under [Authentication](#authentication) as not in version 0.1
