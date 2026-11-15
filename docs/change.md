# Safe change: `svetra diff` and `svetra plan`, version 0.1 (draft)

This document specifies how Svetra tells a person, and their agent, what a change to `svetra.yaml` will do to a running app **before** it reaches the database: which data could be lost, which existing rows or clients could break, and who gains or loses access. When the code and this document disagree, that's a bug in one of them.

**Status: draft for review.** The decisions marked **Open** below need the maintainer's answer before building.

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
svetra diff [NEW] [--from OLD] [--json]
svetra plan [NEW] [--from OLD] [--json] [--accept KIND...]
```

- **`NEW`** defaults to `./svetra.yaml`.
- **`--from OLD`** is the design the database has now. Default: the same path in git's `HEAD` (`git show HEAD:<path>`), because the usual flow is "edit, plan, commit". Outside git, or for a file not in `HEAD`, `--from` is required.
- **Both files must validate.** An invalid one prints its diagnostics and exits `1`.
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
| `add_enum_value` | `status` gains `archived` | safe |
| `remove_enum_value` | `status` loses `frozen` | **breaking**: rows holding it fail the new check |
| `make_required` | `required: false` → `true` | **breaking**: rows with no value fail |
| `make_optional` | `required: true` → `false` | safe |
| `add_constraint`, `add_unique` | `amount_positive`, `unique: [email]` | **breaking**: existing rows may violate it |
| `remove_constraint`, `remove_unique` | | safe for data |
| `add_index`, `remove_index` | | safe |
| `make_private` | `private: true` | **breaking**: the app's API loses the table |
| `add_role`, `remove_role` | `remove_role` also drops every grant of it | `remove_role` is **destructive** for those grants |
| `add_hole`, `change_hole`, `remove_hole` | signatures change the typed client | `remove_hole` and a changed signature are **breaking** for app code calling it |
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
| C4 | A GitHub Action that posts `plan` on PRs | **Yes, after 0.1:** the access lines are exactly what a reviewer should see |

## Tests

- **One fixture pair per kind**, with the expected classification, in `tests/change/`.
- **Golden plans** for the examples: bookings with a rename, a removal and a widened rule.
- **On a real database** (`supabase-check.sh`): a fixture with data, a rename applied through `plan` then `db diff`, and every row still there afterwards. The same change without `was:` must be reported destructive.
- **Access lines agree with `svetra view`:** each wider or narrower line matches the two designs' view models.
