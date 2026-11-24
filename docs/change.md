# Safe change: `svetra diff` and `svetra plan`, version 0.1 (draft)

This document specifies how Svetra tells a person, and their agent, what a change to `svetra.yaml` will do to a running app **before** it reaches the database: which data could be lost, which existing rows or clients could break, and who gains or loses access. When the code and this document disagree, that's a bug in one of them.

**Status: built in 0.6 (unreleased).** The open decisions took the recommendations: `--from` defaults to git `HEAD` (C1); the rename migration's file name uses the clock, or `--at` (C2); `compile` doesn't refuse (C3); the GitHub Action comes later (C4). The MCP `plan` tool is read-only: it never writes the rename migration, which needs `svetra plan --accept` after the person agrees.

**Done when:** a person renames a field and removes an entity on a database that has data, and loses nothing they didn't explicitly agree to lose; every change is classified before it's applied.

## Why

Today a change goes: edit `svetra.yaml`, `svetra compile`, then `supabase db diff` turns the new `supabase/schemas/svetra.sql` into a migration. That works for additions, and it's dangerous for the rest:

- **A rename looks like a drop and an add.** A schema diff tool sees column `customer` gone and column `client` new, and writes `drop column customer; add column client`. Every value is lost, silently.
- **Removals lose data,** and nothing asks first.
- **A new required field, constraint or unique rule** fails the migration on existing rows, or worse, half the deploy succeeds.
- **Access changes aren't visible.** Turning `customer == auth.user` into `true` is one line in a diff; it's also "everyone can now see every booking".

Svetra knows things a schema diff can't: the design before and after, what each name means, and, through the access tests' evaluator, exactly who can see what in both versions.

## Principles

1. **Classify, don't replace Supabase's migrations.** Svetra keeps the declarative schema and `supabase db diff`. It adds what they lack: a plan that classifies every change, and the one migration step a schema diff gets wrong (renames).
2. **Two separate questions per change:** what happens to the **data** (safe, breaking, destructive) and what happens to **access** (wider, narrower, unchanged). A change can be data-safe and still be the most important line in the plan.
3. **Nothing destructive or wider without a yes.** `plan` exits non-zero on destructive data changes and on wider access, until they're acknowledged. Agents must show those lines to the person, in words.
4. **Deterministic.** The same two designs always give the same plan.

## Commands

```sh
svetra diff [NEW] [--from OLD | --from-git REF] [--json]
svetra plan [NEW] [--from OLD | --from-git REF] [--json | --markdown] [--accept KIND...] [--no-write]
```

- **`NEW`** defaults to `./svetra.yaml`.
- **`--from OLD`** is the design the database has now. Default: the same path in git's `HEAD` (`git show HEAD:<path>`), because the usual flow is "edit, plan, commit". Outside git, or for a file not in `HEAD`, `--from` is required.
- **Both files must validate.** An invalid one prints its diagnostics and exits `1`.
- **`--from-git REF`** reads the old design from git `REF` instead of a file, such as `origin/develop` (the PR's base in CI); **`--no-write`** keeps `plan` from writing the rename migration; **`--markdown`** prints the plan as a PR comment. Details and the GitHub Action that uses them: [action.md](action.md).
- **`diff`** lists the changes, classified. It always exits `0` when both files are valid: it's for reading.
- **`plan`** prints the same list as steps, plus what to run, and **exits `1` if any change is destructive or widens access** and isn't named in `--accept`. It writes nothing unless a rename needs a migration (below).

## What counts as a change

Changes are computed between the two typed models, never from text, so reordering keys or reformatting changes nothing. Each change has a stable `kind`.

### Data

| Kind | Example | Data |
|---|---|---|
| `add_entity` | a new `Comment` | safe |
| `add_field` (optional, or with `default` or `auto`) | `notes: { type: text }` | safe |
| `add_field` (required, no default) | `phone: { type: text, required: true }` | **breaking**: existing rows have no value. Hint: add a `default`, or make it optional first and backfill |
| `remove_entity` | `Comment` deleted | **destructive** |
| `remove_field` | `notes` deleted | **destructive** |
| `rename_entity`, `rename_field` | declared with `was:` (below) | safe, with a migration step |
| `change_type`, widening | `int` → `decimal`, `decimal(10,2)` → `decimal(18,2)` | safe |
| `change_type`, anything else | `text` → `int`, scale reduced, `ref` target changed | **destructive**: values can be rejected or rounded |
| `change_on_delete` | `on_delete: restrict` → `cascade` | `cascade` is **breaking**: deleting a row now also deletes the rows that point at it. Any other change is safe |
| `add_enum_value` | `status` gains `archived` | safe |
| `remove_enum_value` | `status` loses `frozen` | **breaking**: rows holding it fail the new check |
| `make_required` | `required: false` → `true` | **breaking**: rows with no value fail |
| `make_optional` | `required: true` → `false` | safe |
| `add_constraint`, `add_unique` | `amount_positive`, `unique: [email]` | **breaking**: existing rows may violate it |
| `remove_constraint`, `remove_unique` | | safe for data |
| `add_index`, `remove_index` | | safe |
| `make_private` | `private: true` | **breaking**: the app's API loses the table |
| `add_audit`, `remove_audit` | `audit: true` added to or removed from an entity or a hole | safe for data. `remove_audit` is listed so it's seen: what happens next is no longer recorded, and the log already written stays |
| `change_require` | a hole's `require` list added, removed or edited | safe for data. Listed so it's seen: a call that used to run may now be refused, or the reverse |
| `change_idempotent` | a hole's `idempotent` key added, removed or changed | safe for data. Listed so it's seen: a repeated call is now answered from the first, or runs again, and the app's retries behave differently |
| `add_role`, `remove_role` | `remove_role` also drops every grant of it | `remove_role` is **destructive** for those grants |
| `add_hole`, `change_hole`, `remove_hole` | signatures change the typed client | `remove_hole` and a changed signature are **breaking** for app code calling it |
| `add_hole` (scheduled) | a `schedule` hole is added | safe |
| `add_hole` (triggered) | a `trigger` hole is added | safe |
| `change_schedule` | a hole's `schedule` changes, or its kind does | **breaking**: it runs at other times with no data change |
| `change_trigger` | a trigger hole's `on`, `events` or `when` changes, or its kind does | **breaking**: it runs after other changes with no data change |
| `remove_hole` (scheduled) | a `schedule` hole is removed | **breaking**: it stops running. Its job is unscheduled by the generated schema while another scheduled hole remains; if it was the last, the hint gives the `cron.unschedule` to run by hand |
| `remove_hole` (triggered) | a `trigger` hole is removed | **breaking**: it stops running. The generated schema no longer creates its triggers, but a database that has them keeps them, so the hint gives the `drop trigger` statements |
| `change_auth` | `password.min_length` 10 → 12 | safe for data; listed so it's seen |

### Access

For both designs, Svetra builds the same table `svetra view` shows: every person × entity × action, answered All, Some or No by the access tests' evaluator. A change **widens** access when any cell goes up (No → Some, No → All, Some → All) and **narrows** it when any cell goes down. Each changed cell becomes one line, in words:

```
wider: A signed-in person can now see some locations (was: no locations).
       Because: Signed-in people can see a location when ...
narrower: staff at a location can no longer change bookings (was: some).
```

- Only people and entities present in both designs are compared; new entities and roles are reported as additions instead.
- A cell the evaluator can't decide is reported as "can't tell", never as unchanged.
- Hole callers are compared the same way.

#### Column rules (`only`)

`only` (docs/column-rules.md) limits which fields an update may change. The view's cells say who may update a row, not which fields, so `only` is compared on its own, for each policy that exists in both designs on the same entity. Each change is one `wider_access` or `narrower_access` line naming the rule and the fields, and `plan` treats it like any other access change:

| Change to a permissive policy | Access |
|---|---|
| gains `only` | narrower: fewer fields can change |
| loses `only` | **wider**: every field can change again; needs `--accept wider_access` |
| a field is added to `only` | **wider**: that field can change |
| a field is removed from `only` | narrower |

A restrictive policy follows the same table: its `only` locks every field outside it, so a shorter list, or gaining one, is narrower, and a longer list, or losing it, is wider.

Fields are matched through `was`, so renaming a field that is in an `only` list is not a change to the list.

## Renames

A rename must be declared, because two designs alone can't tell "renamed" from "removed one, added another":

```yaml
entities:
  Booking:
    fields:
      client: { type: user, required: true, was: customer }
```

- **`was`** is allowed on an entity and on a field. It names what this was called in the design the database has. It must name an entity or field that exists in `--from` and not in the new design (SVT030, new).
- **`plan` writes the rename as its own migration,** before `supabase db diff` runs, so the diff sees a column that's already renamed and writes nothing destructive: `alter table "public"."booking" rename column "customer" to "client";`. Policies, indexes and constraints that mention the old name are recreated by the normal diff.
- **After the change is applied,** `was` is stale; `validate` reports it as info (SVT031, new) with the hint "Remove `was: customer`; the rename is applied." It's harmless to leave.
- An undeclared remove-plus-add of the same type on the same entity gets a hint in the plan: "Did you rename `customer` to `client`? Add `was: customer`, or this loses every value."

## Output

### Human

```
svetra plan: 4 changes to Bookings (from HEAD)

  safe         add field Booking.notes
  rename       Booking.customer -> Booking.client (migration written: supabase/migrations/20261116120000_svetra_rename.sql)
  destructive  remove entity Comment: every comment is deleted
  wider        A signed-in person can now see some locations (was: none).

Not applied: 1 destructive change and 1 wider access change need a yes.
  Re-run with --accept remove_entity,wider_access once the person has agreed.
Then: svetra compile && supabase db diff -f <name> && supabase db push
```

### JSON

```json
{
  "from": "HEAD:svetra.yaml",
  "changes": [
    { "kind": "remove_entity", "path": "entities.Comment", "data": "destructive", "access": null,
      "message": "Removing Comment deletes every comment.", "hint": "Keep it, or confirm with --accept remove_entity." },
    { "kind": "wider_access", "person": "alice", "entity": "Location", "action": "see",
      "was": "none", "now": "some", "data": "safe", "access": "wider",
      "message": "A signed-in person can now see some locations (was: none).",
      "because": ["Signed-in people can see a location when ..."] }
  ],
  "needs_yes": ["remove_entity", "wider_access"],
  "writes": ["supabase/migrations/20261116120000_svetra_rename.sql"]
}
```

## Agents

- **MCP:** a `plan` tool with the same JSON, and `diff` folded into it.
- **The skill** gains a step between validate and compile: *when `svetra.yaml` differs from what's committed, run `plan`; show every destructive and wider line to the person in their words; never pass `--accept` without their explicit yes in this conversation.*

## Not in version 0.1

- `svetra undo`. Reverting the design is `git revert`; the plan then classifies the reverse change like any other. Data deleted by a destructive change can't come back from a design, so `undo` would promise something it can't keep.
- Data migrations (backfilling a new required field from other columns). The plan says what's needed; the agent writes it as a migration by hand.
- Applying anything to a database. `plan` ends with the commands; running them stays with the person (M8, `svetra apply`).
- Renames of policies, roles and holes: they carry no data, so remove-plus-add is fine.

## Open decisions

| # | Question | Recommendation |
|---|---|---|
| C1 | Default `--from`: git `HEAD`, or a snapshot Svetra writes after each successful plan (`.svetra/applied.yaml`)? | **git `HEAD`.** No new state file to drift; works in CI on a PR (`--from origin/develop:svetra.yaml` through a `--from-git REF` form) |
| C2 | The rename migration's file name needs a timestamp, which is a clock. | **Use the clock only for that file name**, never for content; or take `--at YYYYMMDDHHMMSS` for reproducible output in tests |
| C3 | Should `compile` refuse when `plan` would exit 1? | **No.** `compile` only writes the declarative schema; nothing reaches the database until `db push`. Keep the gate in `plan` and the skill |
| C4 | A GitHub Action that posts `plan` on PRs | **Yes, after 0.1:** the access lines are exactly what a reviewer should see. Specified in [action.md](action.md) |

## Tests

- **One fixture pair per kind**, with the expected classification, in `tests/change/`.
- **Golden plans** for the examples: bookings with a rename, a removal and a widened rule.
- **On a real database** (`supabase-check.sh`): a fixture with data, a rename applied through `plan` then `db diff`, and every row still there afterwards. The same change without `was:` must be reported destructive.
- **Access lines agree with `svetra view`:** each wider or narrower line matches the two designs' view models.
