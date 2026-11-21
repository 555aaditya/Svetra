# `svetra infer`: draft a `svetra.yaml` from an existing database, version 0.1

This document specifies `svetra infer`, which reads an existing Supabase (or plain PostgreSQL) database and writes a draft `svetra.yaml` for it. It's the step after [`svetra check`](check.md): `check` finds the leaks and leaves a list, and `infer` gives you a file a person can adopt, so the fixes become generated, tested rules. When the code and this document disagree, that's a bug in one of them.

**Milestone 1 (this document) drafts the data model only:** entities, fields, constraints, unique constraints and indexes, plus `check`'s findings as comments. It writes no policies and no roles. Those are milestone 2 (see [Policies, later](#policies-later)).

**Done when:** `svetra infer --snapshot` on the sample app's snapshot writes a draft that loads and validates with no errors, keeps every field it can translate exactly, and lists every field, constraint or index it can't as an `UNTRANSLATED` comment with the original SQL.

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
# Every entity is `private: true`: no policies are inferred yet, so nothing is exposed through
# the API until you add policies and remove `private`. That differs from the live database.
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

- **Every entity has `private: true`.** The draft has no policies, and `svetra validate` makes an exposed entity without a policy an error (`SVT001`). Private is the safe reading: nothing is exposed until you write a rule. This changes what an adopted draft does compared with the live database (the API stops reaching these tables), and the header says so.
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

A draft has no errors. `svetra validate` still reports `SVT004` (warning) on every entity, because nothing can write a table that has no policy, and `SVT009` (info), because there is no `auth` section; `SVT010` (info) appears for a reference with no index. All three go away as you add policies, `auth` and indexes.

## Policies, later

Milestone 1 writes no `policies:` and no `roles:`, because inferring them is the part where a wrong guess leaks or locks out data. The plan, so that the first milestone's layout doesn't get in its way:

- **Owner-column policies.** A policy `using (auth.uid() = owner)` on a table with a `user` field `owner` becomes `when: "owner == auth.user"`, per action. Only conditions that translate exactly are written; anything else stays a `# UNTRANSLATED policy` comment with its SQL, as with constraints.
- **Membership tables and roles.** A table with two references, one to a `user` and one to an entity, and policies that look it up, suggests a scoped role. Proposed as a comment first, never written silently.
- **Differential testing.** Compile the draft, run its generated access tests against the original database, and report each person-and-row check where the draft and the live database disagree. That is how a person knows the draft means what the database does.
- When policies arrive, `private: true` is dropped for entities the draft gives a policy.

## Not in version 0.1

- Policies, roles, holes and `auth` settings.
- Views, functions, triggers, sequences, comments on objects, extensions, grants and storage buckets.
- Composite primary keys and foreign keys, domains, arrays, ranges, generated columns, and identity as a real auto-increment.
- Singular entity names and renames (`was`).
- Following a foreign key to a table outside the listed schemas, other than `auth.users`.
- An MCP tool for `infer`.
