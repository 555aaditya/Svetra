# `svetra infer`: draft a `svetra.yaml` from an existing database, version 0.2

This document specifies `svetra infer`, which reads an existing Supabase (or plain PostgreSQL) database and writes a draft `svetra.yaml` for it. It's the step after [`svetra check`](check.md): `check` finds the leaks and leaves a list, and `infer` gives you a file a person can adopt, so the fixes become generated, tested rules. When the code and this document disagree, that's a bug in one of them.

**Milestone 1 drafts the data model:** entities, fields, constraints, unique constraints and indexes, plus `check`'s findings as comments. **Milestone 2 adds policies,** starting with the owner column (see [Policies](#policies)); it writes no roles. Where this document says "milestone 2 (not built yet)", the code doesn't do it yet.

**Done when:** `svetra infer --snapshot` on the sample app's snapshot writes a draft that loads and validates with no errors, keeps every field it can translate exactly, and lists every field, constraint, index or policy it can't as an `UNTRANSLATED` comment with the original SQL. **For policies:** the draft is never wider than the database. Whatever it can't translate exactly is left out, and the draft says that it is narrower.

## Command

```sh
svetra infer --db URL        [--schema public,...] [--out FILE] [--force]
svetra infer --local         [--schema public,...] [--out FILE] [--force]
svetra infer --snapshot FILE [--schema public,...] [--out FILE] [--force]
svetra infer --print-query   [--schema public,...]
```

- **`--db`, `--local`, `--snapshot`, `--print-query`** work as in `check`. `--print-query` prints the query for the Supabase SQL editor; save the one JSON value it returns and pass it with `--snapshot`.
- **`--schema`** lists the schemas to read. Default: `public`. Svetra's tables live in `public` (docs/format.md), so only tables in `public` become entities. A table in another listed schema is reported as a comment and skipped.
- **`--out FILE`** is where the draft goes. Default: `svetra.yaml`. `--out -` prints it to standard output and writes nothing else.
- **`--force`** overwrites `FILE`. Without it, `infer` refuses when `FILE` exists, so a design you've already edited is never lost.
- **Exit codes:** `0` the draft was written, `2` a usage error, an unreachable database, a snapshot without the new keys, or a file that exists. Findings never change the exit code, because the draft is the result.

Reading is read-only: one query, no writes.

## What it reads

The snapshot is the one `check` reads, extended with four **new keys**. The existing keys (`tables`, `policies`, `views`, `functions`) are unchanged, and `svetra check --print-query` prints the same query as before. `svetra infer --print-query` prints the same query with the four keys added, so a snapshot made for `infer` also works with `check --snapshot`. A snapshot without the new keys is refused by `infer` with "That snapshot predates `infer`. Run `svetra infer --print-query` and take it again."

All four cover ordinary tables in the listed schemas (not views, not partitions of another table).

| Key | For each | Fields |
|---|---|---|
| `columns` | column of a table (`pg_attribute`, not dropped, not a system column), in column order | `schema`, `table`, `name`, `type` (`format_type`, such as `numeric(18,6)`, `character varying(255)`, `timestamp with time zone`), `type_schema` and `type_name` (the type's own name, to find an enum), `kind` (the type's `typtype`: `b` base, `e` enum, `d` domain, and so on), `not_null`, `default` (the deparsed expression, or `null`), `identity` (`always`, `by default` or `null`), `generated` (`true` for a generated column) |
| `constraints` | constraint on those tables (`pg_constraint`): primary key, foreign key, unique, check | `schema`, `table`, `name`, `kind` (`p`, `f`, `u`, `c`), `columns` (the constrained columns, in order), `definition` (`pg_get_constraintdef`), and for a foreign key `ref_schema`, `ref_table`, `ref_columns` |
| `indexes` | index on those tables that backs no constraint (`pg_index`) | `schema`, `table`, `name`, `unique`, `method` (`btree`, `gin`, ...), `columns` (names; `null` where the position is an expression), `partial` (has a `where`), `definition` (`pg_get_indexdef`) |
| `enums` | enum type in a listed schema (`pg_enum`) | `schema`, `name`, `values` in sort order |

- Primary keys come from `constraints` (`kind` `p`), not from a column flag.
- Rows are ordered by schema, table and position, so the same database gives the same snapshot.
- Tables the new keys don't describe (partitions, which `tables` still lists) are skipped and named in a comment.

## What it skips

- **Supabase's own schemas** (`auth`, `storage`, `realtime`, `extensions`, `graphql`, `graphql_public`, `vault`, `supabase_*`, `pgsodium*`, `pg_*`, `information_schema`) are never read as tables. `auth.users` is only a foreign-key target (see `user` below).
- **Views, materialized views and functions** are not entities. They are not translated and not listed individually; `check`'s findings about them still appear in the findings comments. Their count is stated in the header comment.
- **Partitions** of a partitioned table, and **tables outside `public`**: named in a comment, skipped.
- **Triggers, sequences, grants, extensions, comments on objects:** not read.

## Naming

- **The system** is named `App`. The snapshot doesn't know the application's name; rename it.
- **An entity's name is its table name in PascalCase, with no singularizing:** `order_items` becomes `OrderItems`, `profiles` becomes `Profiles`. The compiler writes an entity's table as its name in snake_case (docs/format.md), so only this mapping compiles back to the same table. Singularizing (`OrderItem`) would compile to `order_item`, a different table, and `svetra plan` would see a rename. If you want singular names, rename an entity and add `was`, then plan it.
- **A table Svetra can't name** is left as a comment: its name isn't made of lowercase letters, digits and underscores starting with a letter, or its PascalCase name doesn't compile back to the same table (for example `Order` with a capital, or `a__b`).
- **A field's name is its column name.** A column whose name isn't snake_case (`^[a-z][a-z0-9_]*$`) is left as a comment. Reserved words and quoted names follow the same rule.
- **Constraint names** are kept when they are snake_case and unique across the file; otherwise `<table>_<column>_check` style names are generated, numbered when needed. Constraint names must be unique across the file (docs/format.md, `SVT024`).

## Mapping, and how exact it is

Each row says what a column becomes and whether the draft means exactly what the database has. **Exact** means the compiled column is the same. **Widened** means the compiled column can hold more than the original. Anything else is left explicit.

| PostgreSQL | Svetra field | Fidelity |
|---|---|---|
| `uuid`, as the primary key | `{ type: uuid, primary: true }` | Exact. The default `gen_random_uuid()` or `uuid_generate_v4()` is implied by Svetra and dropped |
| `uuid` | `uuid` | Exact |
| `text`, `varchar(n)`, `char(n)`, `citext` | `text` | Exact for `text`. A length limit is dropped: **widened**, and a `# lossy` note says so |
| `boolean` | `bool` | Exact |
| `date` | `date` | Exact |
| `timestamptz` | `timestamp` | Exact |
| `timestamp` (without time zone) | `timestamp` | **Changed**: the compiled column has a time zone. A note says so |
| `jsonb`, `json` | `json` | Exact for `jsonb`. `json` becomes `jsonb` (note) |
| `smallint`, `integer`, `bigint` | `int` | **Widened** to 64 bits (note). An integer primary key stays a primary key |
| `numeric(p,s)`, 1 to 38 digits | `money` when the column name has a money word (`amount`, `price`, `balance`, `cost`, `total`, `fee`, as a whole word between underscores), otherwise `decimal`, with `precision` and `scale` | Exact |
| `numeric` without precision, or over 38 digits | not translated | `UNTRANSLATED` |
| `real`, `double precision` | `float` | `real` is **widened** to 64 bits (note) |
| an enum type | `{ type: enum, values: [...] }` | Values exact. The compiled column is `text` with a `check`, not the enum type (note) |
| foreign key to a table that becomes an entity with a `uuid` primary key | `{ type: ref, entity: X }` | Exact, when the key is one column pointing at the target's primary key |
| foreign key to `auth.users (id)` | `{ type: user }` | Exact |
| any other foreign key, including one on the primary key itself | the column's own type, plus a comment | `UNTRANSLATED` foreign key with its SQL, at the end of the entity |
| `real` or `double precision` on a column named like money | not translated | `UNTRANSLATED`: Svetra refuses float for money (`SVT005`), so the draft would not validate. Choose `numeric(p,s)` |
| arrays, ranges, `bytea`, `inet`, `tsvector`, `interval`, domains, extension types (`vector`, `geometry`), generated columns | not translated | `UNTRANSLATED`, the column is left out of the entity |
| identity column or `serial` (a `nextval` default) | the column's type, plus a comment | `UNTRANSLATED` identity or default: Svetra has no auto-increment, so inserts must give the value |

The `ref`'s `on delete` and `on update` actions are not part of Svetra's foreign keys (it writes none). A foreign key that has one gets a `# lossy` note with the original.

**Modifiers**

| Database | Svetra | Fidelity |
|---|---|---|
| `not null` | `required: true` (omitted for the primary key, which implies it) | Exact |
| default that is a literal: `'x'::text`, `0`, `true`, `1.50` | `default: <value>` | Exact |
| default `now()`, `CURRENT_TIMESTAMP` on a timestamp; `CURRENT_DATE` on a date | `auto: now` | Exact |
| any other default (a function, an expression, a `nextval`) | nothing, plus a comment | `UNTRANSLATED` default with its SQL |
| single-column unique constraint, or unique index with no `where` | `unique: true` | Exact |
| multi-column unique constraint or unique index | the entity's `unique:` list | Exact |
| non-unique `btree` index on translated columns, no `where` | the entity's `indexes:` list | Exact |
| other indexes (`gin`, `gist`, partial, expression, over an untranslated column) | a comment | `UNTRANSLATED` index with its SQL |
| check constraint | the entity's `constraints:` map, when the expression translates (below); otherwise a comment | See below |
| primary key on one column | `primary: true` | Exact |
| composite primary key, or none | the table is left as a comment | `UNTRANSLATED` table: an entity needs exactly one primary field |

**A column that can't be translated is left out of the entity, never dropped silently:** the entity's comment block lists it with the original type and SQL. Because it's left out, a `not null` column the app depends on would be missing from the compiled table. The comment says so, and the summary line counts it.

### Check constraints

A check constraint is translated only if it comes out as an expression that Svetra's expression language parses (docs/format.md, Expressions) and that type-checks against the entity's fields. `infer` does this in small, fixed steps, and no others:

1. Drop `::type` casts, and brackets that change nothing (around one token, or around the whole expression). Dropping a cast can change what an expression means (`stock::text <> '-1'`), which is why step 6 exists.
2. `AND`, `OR`, `NOT` become `and`, `or`, `not`. `=` becomes `==`, `<>` becomes `!=`.
3. `x IS NULL` and `x IS NOT NULL` become `x == null` and `x != null`.
4. `x = ANY (ARRAY['a', 'b'])` becomes `x in ['a', 'b']`.
5. Parse the result. A column in it must be one of the entity's translated fields. Functions, `~`, `LIKE`, `||`, `::` into other types than those dropped, and subqueries don't parse, so the constraint is left as a comment.
6. After the whole draft is built, it's validated. A constraint that fails to type-check (`SVT003`) is moved to a comment too, so the draft never carries a constraint that doesn't validate.

A translated constraint keeps the database's meaning. **Null behaves as in SQL** in both languages (docs/format.md), so no `or x is null` is needed. A constraint that isn't translated is written as

```yaml
    # UNTRANSLATED check "positive_qty": CHECK ((qty > 0 AND qty < least(max_qty, 100)))
```

## Output

One file. It starts with a header comment, then the findings, then the YAML.

```yaml
# Drafted by `svetra infer` from a database snapshot. Review before adopting.
# Policies: 2 of 3 translated (see Policies in docs/infer.md). The draft is never wider than the
# database; an entity is `private: true` only when none of its policies translated.
# 14 entities from 14 tables. Skipped: 3 views, 6 functions.
# 5 things left as UNTRANSLATED (search for UNTRANSLATED).
#
# svetra check found 2 critical, 3 warnings and 1 note:
#   critical  public.orders: Anyone on the internet can read every row of public.orders: row-level security is off.
#   ...
# yaml-language-server: $schema=https://raw.githubusercontent.com/555aaditya/Svetra/main/schema/svetra.schema.json
svetra: "0.1"
system: App   # rename

entities:
  Orders:
    # check: critical, rls_off: Anyone on the internet can read every row of public.orders.
    private: true
    fields:
      id:         { type: uuid, primary: true }
      customer:   { type: user, required: true }
      total:      { type: money, precision: 12, scale: 2, required: true }
      status:     { type: enum, values: [open, paid], default: open }   # lossy: was an enum type, compiles to text with a check
      qty:        { type: int }   # lossy: was integer, compiles to bigint
      # UNTRANSLATED column tags text[]
    constraints:
      total_non_negative: "total >= 0"
    # UNTRANSLATED check qty_range: CHECK (((qty > 0) AND (qty < least(max_qty, 100))))
```

Exact rules:

- **An entity is `private: true` exactly when none of its policies translated.** `svetra validate` makes an exposed entity without a policy an error (`SVT001`), and private is the safe reading: nothing is exposed until you write a rule. For such an entity the draft differs from the live database (the API stops reaching the table), and a comment on the entity says why. The header counts policies translated and not (`# Policies: N of M translated`).
- **Entities are in table-name order,** fields in column order, so the output is deterministic and reviewable. No clocks, no hash order.
- **Entity keys, in order:** finding comments, `private`, `fields`, `constraints`, `unique`, `indexes`, then the entity's `UNTRANSLATED` comments for checks, unique constraints, indexes and foreign keys. An untranslated column's comment sits in `fields` at its place; a default's or identity's note is at the end of that field's line.
- **Comments:** `# UNTRANSLATED <what> <name>: <original SQL>` for anything left out, always with the original SQL on one line. `# lossy: ...` for a widened or changed translation. `# check: <severity>, <kind>: <message>` for a finding on that table, above `private`.
- **Findings** are `check`'s, unchanged. Those about a table are repeated above that entity; all are listed in the header, most severe first, as one line each (`severity  object: message`). Findings about views and functions only appear in the header. Fixes are not repeated: run `svetra check` for them.
- **Text in comments is made safe:** newlines in SQL are collapsed to single spaces, so a comment can't end early or inject a key.
- **A table left as a comment** appears at the end, under `# Tables left out of the entities:`, one `# UNTRANSLATED table <schema.name>: <reason>` line each.

It prints, and writes the file only if `--out` isn't `-`:

```
wrote svetra.yaml: 14 entities, 97 fields, 5 things left as UNTRANSLATED, 2 critical findings in comments
Next: review the draft, then `svetra validate`, then `svetra compile`.
```

With nothing left out it says `0 things left as UNTRANSLATED`. The header counts words as they are: `1 view`, `0 functions`. The counts are the entities and fields in the file, the comments beginning `# UNTRANSLATED`, and the critical findings (the other severities are in the file but not repeated here).

### What `validate` says about a draft

A draft has no errors. `svetra validate` still reports `SVT004` (warning) on every entity nothing can write (no translated insert or update policy), and `SVT009` (info), because there is no `auth` section; `SVT010` (info) appears for a reference with no index. All three go away as you add policies, `auth` and indexes.

## Policies

Policies are where a wrong guess leaks or locks out data, so the rule for this whole section is: **the draft is never wider than the database.** When a translation is exact, it's written. When it isn't, or when `infer` isn't sure, the policy stays an `UNTRANSLATED` comment, and the effect is that the draft lets fewer people in than the database does. Locking someone out is a bug you notice; letting someone in is a leak you don't.

`infer` reads `policies` (`pg_policies`) and the `rls` flag and privileges in `tables`, all already in the snapshot. No new keys, so a snapshot that works for milestone 1 works for this.

### One database policy, one Svetra policy

Each `pg_policies` row becomes **at most one** Svetra policy, with the same name when that name is snake_case and not already used (otherwise `<table>_<name>`, numbered if needed). `infer` does not merge policies that share a condition, and does not split one into several. The reasons: the database's own structure survives, so a person can compare the two files line by line; permissive and restrictive policies stay what they were; and merging is a guess about intent that gains nothing, because Svetra compiles each action to its own database policy anyway (docs/format.md, Policies). Policies are written in a top-level `policies:` section, in table-name order, then by database policy name.

### Commands: `allow`, `when`, `check`

| Database policy | Svetra | Notes |
|---|---|---|
| `FOR SELECT USING (c)` | `allow: [select], when: c'` | `with check` is always null |
| `FOR INSERT WITH CHECK (c)` | `allow: [insert], when: c'` | `using` is always null. `when` is the check on the new row (docs/format.md) |
| `FOR DELETE USING (c)` | `allow: [delete], when: c'` | |
| `FOR UPDATE USING (u)` | `allow: [update], when: u'` | No `with check`: the database uses `u` for the new row too, and so does Svetra |
| `FOR UPDATE USING (u) WITH CHECK (k)` | `allow: [update], when: u', check: k'` | The old row must pass `u`, the new row `k` (docs/update-check.md). `check` is left out when `k'` is `u'` |
| `FOR ALL USING (u)` | `allow: [select, insert, update, delete], when: u'` | Also with `WITH CHECK (k)` when `k'` is `u'` |
| `FOR ALL USING (u) WITH CHECK (k)` with different conditions | not translated | One Svetra policy can't say "insert on `k`, select and delete on `u`". Writing one policy per command is the person's call |

`c'` is the condition translated as below. An update policy with no `using` (only `with check`) is not translated: how the database treats the old row there is subtle, so `infer` doesn't guess. A condition that doesn't translate makes the whole database policy untranslated; there is no half-translation inside one policy.

**Roles become `as`.** `to authenticated` (what the Supabase dashboard writes) is `as: authenticated`, which is also Svetra's default, so it's written out for readability. `to anon` is `as: anon`. `to public` (the default when there is no `to`) and `to anon, authenticated` are `as: anyone`. Any other role (`service_role`, `postgres`, a custom role), alone or in a mix: not translated. `as: anon` still gets Svetra's own warning (`SVT007`) when signed-in people would get less.

**Permissive and restrictive.** A `RESTRICTIVE` database policy becomes `restrictive: true`; a permissive one writes nothing (docs/update-check.md).

**Privileges.** Row-level security is only one gate: the role also needs the table privilege. A translated policy lists an action only when the database grants that privilege (`tables[].anon` and `.authenticated`, table level) to every role the policy's `as` covers. An action the database withholds is dropped from `allow`, and a policy left with no action is not translated. This keeps the draft from allowing what a `revoke` blocked. (A database without the `anon` and `authenticated` roles reports every privilege as false, so on plain PostgreSQL nothing translates. Column-level grants aren't read: a column-level revoke narrows the database, so the draft can be wider there. Review them if you use them.)

### Conditions

`infer` reads a condition the way `pg_policies` prints it, and translates **only these shapes**, joined by `AND` and `OR` and nested in brackets:

| Database | Svetra | Notes |
|---|---|---|
| `true` | `true` | Whoever the role is: with `to anon`, `to authenticated` or `to public`, the matching `as:` and `when: "true"` |
| `col = auth.uid()`, `auth.uid() = col` | `col == auth.user` | `col` must be one of the entity's **`user` fields** (a `uuid` column with a foreign key to `auth.users (id)`). A plain `uuid` column, or a column that didn't translate, is not an owner |
| `col = (select auth.uid())`, `(select auth.uid()) = col`, and the form the database prints, `col = ( SELECT auth.uid() AS uid)` | `col == auth.user` | The wrapped form Supabase recommends, for speed. Same meaning |
| `(col)::text = (auth.uid())::text`, either side first | `col == auth.user` | A cast to `text` on **both** sides compares the same values as the uuids, so it is exact. A cast on one side only, or any other cast, is not translated |
| `(auth.uid())::uuid = col` | `col == auth.user` | A cast to `uuid` of a uuid is a no-op, so it is dropped |
| `a AND b`, `a OR b` | `a and b`, `a or b` | Only when each of `a` and `b` translates. Brackets are kept where they matter; null behaves as SQL in both |

Everything else is **not translated**, with no attempt to be clever. That includes: a subquery (`exists (select ...)`, `col in (select ...)`, any `SELECT` that isn't `auth.uid()`), a membership table, `auth.jwt()` and its claims (`auth.jwt() ->> 'role'`, `app_metadata`), `auth.role()`, `current_user`, any function call (your own `is_admin()` helpers included), `NOT`, `IS NULL`, `<>`, comparisons with literals (`status = 'published'`), `col = auth.uid()` where `col` is not a `user` field, and `false`. Some of these have a Svetra equivalent a person could write by hand (`has_role`, a one-hop path); the draft doesn't guess which. `infer` doesn't read function bodies.

### What the entity says

Per table in `public` that became an entity:

| Database | Entity in the draft |
|---|---|
| Row-level security **on**, at least one translated **permissive** policy | **No `private`.** The translated policies are in `policies:`. If some of the table's policies were not translated, a comment above `fields:` says: `# policies: 1 of 3 translated. The draft is NARROWER than the database: whatever an UNTRANSLATED policy below lets in, this draft keeps out, until you port it.` |
| Row-level security **on**, no policy translated | `private: true`, with a comment saying none of the table's policies translated |
| Row-level security **on**, no policies | `private: true`, as before. Row-level security on with no policy means nobody gets in through the API; private says the same |
| Row-level security **off** | `private: true`, with a comment: `# row-level security is off here, so the database has no access rule (see the check finding). No policy is invented; add one deliberately.` Any policies the table has are listed as `UNTRANSLATED`, because they aren't enforced |

An entity is `private: true` **only** when none of its policies translated.

**A restrictive policy that doesn't translate removes every policy of its table.** Dropping a permissive policy only narrows access, but dropping a restrictive one **widens** it. If any restrictive policy on a table is not translated, none of the table's policies are written (the entity is `private: true`, and each policy is an `UNTRANSLATED` comment with the reason). The same applies when only restrictive policies translated: with no permissive one nobody gets in (`SVT001`), so they stay comments too.

Untranslated policies are comments at the end of the entity, one per policy, with the original SQL rebuilt from the snapshot and the reason after `--`:

```yaml
    # UNTRANSLATED policy staff_read: CREATE POLICY staff_read ON public.orders AS PERMISSIVE FOR SELECT TO authenticated USING (is_staff()) -- a condition that isn't an owner check or `true`
```

### Example

```yaml
entities:
  Orders:
    # policies: 1 of 2 translated. The draft is NARROWER than the database: ...
    fields:
      customer: { type: user, required: true }
      ...
    # UNTRANSLATED policy staff_read: CREATE POLICY staff_read ON public.orders ... -- ...

policies:
  orders_own:
    on: Orders
    as: authenticated
    allow: [select, update, delete]
    when: "customer == auth.user"
```

### What this does not do

- **Roles.** No `roles:`, no `has_role` and no scoped role is written. Membership tables and role helper functions stay `UNTRANSLATED`. A later milestone may propose them as comments (never write them silently).
- **Holes, `auth` settings, column rules (`only`).** Not inferred. A database trigger that guards columns is not read.
- **Views and functions,** apart from `check`'s findings.

### Differential testing (planned, not built)

Not built yet: it needs pgTAP in the throwaway database, which `scripts/pg-check.sh` installs from the network. When built:

The check that a draft means what the database does is to compile it, run its generated access tests (docs/compiler.md) against the **original** database, and look at each person-and-row check where they disagree. For a table whose policies **all** translated, the original database must pass every check the draft's tests make; a failure is a translation bug. Where the draft is narrower on purpose (an untranslated policy), the original allows more than the draft and the tests may fail, which is the expected disagreement, so that table is not part of the check. It would be a test in the project, not a user command.

## Not in this version

- Roles, holes and `auth` settings. Policies translate only for the shapes in [Policies](#policies).
- Views, functions, triggers, sequences, comments on objects, extensions, grants and storage buckets.
- Composite primary keys and foreign keys, domains, arrays, ranges, generated columns, and identity as a real auto-increment.
- Singular entity names and renames (`was`).
- Following a foreign key to a table outside the listed schemas, other than `auth.users`.
- An MCP tool for `infer`.
