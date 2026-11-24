# `svetra patch`: checked edits to the design, version 0.1 (draft)

This document specifies `svetra patch`, the way an agent changes `svetra.yaml`. Instead of rewriting the file, the agent sends a short list of edits. Svetra applies all of them, checks the result, and writes the file **only if the result is valid**. A bad edit fails before anything is written, and says which edit and why.

**Status: built in 0.9 (unreleased).** The decisions at the end took the recommendations (P2, editing the text in place, confirmed by the maintainer on 2026-11-24).

**Done when:** an agent adds a field, renames another and removes an entity in one call; the file keeps its comments and key order; a stale call, based on an older file, is refused and says so; and a call that would leave an invalid design writes nothing.

## Why

- **An agent that rewrites a whole file spends tokens on the parts that didn't change,** and can silently change them. A 20-line edit list costs a fraction of the file, and it is exactly what a reviewer wants to see.
- **A text edit fails late.** A misspelt key or a bad type is found by `validate` after the file is already wrong. A patch is checked as a whole before it is written.
- **Two agents, or an agent and a person, can edit at once.** The second writer overwrites the first without knowing. A patch names the file it was written against, and is refused if the file has moved.

## Command

```sh
svetra patch [FILE] [--ops FILE | -] [--expected-revision REV] [--dry-run] [--json]
```

- **`FILE`** defaults to `./svetra.yaml`.
- **The edits** are a JSON array, from `--ops FILE`, or from standard input with `-` (the default when input is piped).
- **`--expected-revision REV`** is the revision of the file the edits were written against: the first 12 hex digits of the SHA-256 of its bytes, the same number `compile` writes in its header and `svetra context` prints. If the file's revision is different, nothing is applied (see Conflicts). Optional on the command line; the skill always passes it.
- **`--dry-run`** does everything except write.
- **`--json`** prints the result as JSON (see Output).

**Exit codes:**

| Code | When |
|---|---|
| `0` | Applied (or, with `--dry-run`, would apply) |
| `1` | Refused: an edit couldn't be applied, or the result has errors. Nothing written |
| `2` | Usage error: bad JSON, unreadable file, an unknown option |
| `3` | Conflict: the file's revision isn't `--expected-revision`. Nothing written. Safe to retry after re-reading |

## The edits

Three operations, all on a **path** into the file: dot-separated names, exactly as the loader and the diagnostics write them (`entities.Booking.fields.notes`, `policies.own_rows.when`, `holes.close.reads`).

| Edit | Meaning |
|---|---|
| `{"op": "set", "path": P, "value": V}` | Make the value at `P` be `V`. Creates it, and any maps on the way, when it isn't there. Replaces it when it is |
| `{"op": "remove", "path": P}` | Delete the key at `P` and everything under it. An error when `P` isn't there |
| `{"op": "rename", "path": P, "to": N}` | Rename the key at `P` to `N` and, for an entity or a field, add `was: <old name>`, so `plan` sees a rename and keeps the data ([change.md](change.md)). Other keys (a policy, a hole, a role) are renamed with no `was`: they carry no data |

`V` is any JSON value; a JSON object becomes a map, an array a list, and a string stays a string. It is written in the form the file already uses around it (see Keeping the file). **Paths go through maps only:** a segment that lands on a list or a scalar is an error, and a list is replaced whole (`set holes.close.reads ["Order"]`).

**`rename` changes the key and nothing else.** References to the old name (a `ref` field's `entity`, a policy's `on`, a hole's `reads`, an expression that uses the field) are not rewritten. The result is validated as a whole, so every reference left behind is reported, and the agent fixes each with a `set` in the same call, after the `rename`. An existing `was` is kept, not replaced: it names what the database has.

Edits apply **in order, to the result of the one before**, so `set` an entity and then `set` a policy on it works in one call.

Why three, and not `add_field`, `change_type`, `add_policy` and so on: every one of those is a `set` or a `remove` at a path, and the format's own diagnostics already speak in paths. A new operation per kind would have to be learned, kept in sync with the format, and tested; three operations don't.

```json
[
  {"op": "set", "path": "entities.Booking.fields.notes", "value": {"type": "text"}},
  {"op": "rename", "path": "entities.Booking.fields.customer", "to": "client"},
  {"op": "remove", "path": "entities.Comment"},
  {"op": "remove", "path": "policies.all_comments"}
]
```

## Checking

All edits are applied to the file's text in memory, then the **whole result** goes through the same checks as `svetra validate`. If any edit can't be applied, or the result has an error, **nothing is written** and the answer lists every problem found:

- An edit that can't apply: the path doesn't exist for `remove` or `rename`, or its parent is a list or a scalar where a map is needed, or `rename` to a name that's already there. It is reported with its position in the edit list (`ops[2]`) and a hint, and the other edits are still tried so one call lists everything wrong.
- An error in the result: the usual diagnostic, with its code, its position **in the file as it would be written**, and its hint. Warnings and info that the edits bring are shown, as `notes`, and don't stop the write; ones the file already had are not repeated.

The edit that caused a diagnostic isn't traced back; the position says where in the new file, and a `--dry-run` lets the agent see the file it would get.

## What the answer shows

On success, the answer is the same classification `svetra diff` gives, between the file as it was and as it is now (left out, and said so, when the file as it was doesn't validate, since there is then nothing sound to compare): each change as safe, breaking or destructive, and which access widens or narrows, in words. **Patch never refuses a destructive or wider change.** That gate stays in `plan` and in the person's yes, as `compile` stays free to write ([change.md](change.md), C3); the patch only makes sure the agent sees the line in the same answer that applies it. It also gives the new revision.

```
svetra patch: 4 edits applied to svetra.yaml (revision 3f9a1c2e8b7d -> 9d0c47aa1b22)

  safe         add field Booking.notes
  rename       Booking.customer -> Booking.client
  destructive  remove entity Comment: every comment is deleted
  wider        A signed-in person can now see all bookings (was: some).

Show the destructive and wider lines to the person before relying on this. Next: svetra plan.
```

### JSON

```json
{
  "applied": true,
  "dry_run": false,
  "revision": { "was": "3f9a1c2e8b7d", "now": "9d0c47aa1b22" },
  "problems": [],
  "changes": [ { "kind": "remove_entity", "path": "entities.Comment", "data": "destructive", "access": null, "message": "..." } ],
  "needs_yes": ["remove_entity", "wider_access"]
}
```

`changes` and `needs_yes` are `plan`'s, unchanged. When refused, `applied` is `false`, `revision.now` is absent, and `problems` holds each problem: `{ "at": "ops[2]" | "svetra.yaml:14:7", "code": "SVT002" | "edit", "message": "...", "hint": "..." }`.

## Conflicts

A conflict is not an error in the design, and a retry is the right reaction. The answer:

```json
{ "applied": false, "conflict": true, "revision": { "expected": "3f9a1c2e8b7d", "now": "5b61e0c93a17" },
  "hint": "svetra.yaml changed since you read it. Read it again with svetra context, redo your edits against it, and send them with the new revision." }
```

The check happens **after** the file is read and **just before** the write, with the write itself done by writing a temporary file next to it and renaming it over the original, so a change made while the patch was being checked is caught, not overwritten. (Two writers in the same instant can still race; the rename is atomic, the check-then-rename is not. This is a guard for agents that take turns, not a lock.)

## Keeping the file

A person reads `svetra.yaml` in review, so a patch must change **only what the edits say**:

- **Comments, blank lines, key order and quoting stay.** The edits are made on the text, at the positions the parser reports, and not by loading the file and writing it back.
- **`set` on a value that exists** replaces just that value, in place, keeping the key.
- **`set` on a key that doesn't exist** adds it after the last key of its parent, indented like its siblings. In a parent written in flow style (`{ type: text }`), the key is added inside the braces and the parent stays on its line.
- **`remove`** deletes the key's lines, and the comment lines directly above it with no blank line between. In flow style it removes just the entry.
- **`rename`** changes the key's text only. The `was` key is added as a new key, as above.
- **A new map value follows its siblings.** If the first sibling that is a map is written in flow style (`{ type: text }`), the new one is too; if it is written in block style, so is the new one. With no sibling to follow, a map whose values are all scalars or lists is written in flow style, and any other map in block style. Under a flow parent everything is flow.
- **A list is always written in flow style** (`[a, b]`), as the format's own examples do.
- **Strings are plain when they are a simple name or dotted name** (`text`, `Account.id`) **and quoted otherwise**, so an expression is always `"quoted"`. A string that looks like a number, boolean or null is quoted, so it stays a string.
- **A flow map or list that runs over several lines** is edited like any other flow collection: the new entry goes just after the last one. Block scalars (`|`, `>`) are not used by the format and are replaced whole.
- **Nothing is written when the result equals the original** (applied: true, no changes, same revision).

## Agents

- **MCP:** a `patch` tool: `{ file?, ops, expected_revision?, dry_run? }`, answering with the JSON above. Findings, conflicts and problems are results, not tool errors (as for `check`); only a call that can't run is an error.
- **The skill** step 1 changes from "turn the request into `svetra.yaml`" to: *for a new design, write the file; for a change to an existing one, read `context`, note its revision, send a `patch` with `expected_revision`, and show every destructive and wider line in the answer to the person before going on. On a conflict, read again and redo.*
- **`svetra context` prints the revision** on its first line, so the agent has it without another call.

## Not in version 0.1

- Edits to hole bodies (`supabase/holes/*.sql`): those are code, and the agent edits them as files.
- Moving or reordering keys, and editing comments.
- Patterns or wildcards in paths (`entities.*.private`).
- Several files, and a patch that applies to the old design from git: `patch` edits the file as it is now.
- `svetra undo`: reverting is `git revert`, as in [change.md](change.md).
- Writing anything but `svetra.yaml`. Compiling stays a separate step.

## Open decisions

| # | Question | Recommendation |
|---|---|---|
| P1 | One generic `set` / `remove` / `rename` on paths, or named operations (`add_field`, `change_type`, ...)? | **Three generic ones.** Fewer things to learn and to keep in sync; paths are already how every diagnostic speaks |
| P2 | Edit the text in place (keeps comments, the diff stays small), or load and write the file back (simpler, loses comments and formatting)? | **In place** (decided). A reviewed file that is reformatted on every agent edit is worse than the extra code. The parser reports only start positions, so the end of an entry comes from indentation (block style) or from scanning to the matching bracket (flow style) |
| P3 | Should `patch` refuse a destructive or wider change until it's accepted, as `plan` does? | **No.** It reports them in the same answer. The gate stays at `plan`, where the person's yes already lives |
| P4 | Is `expected_revision` required? | **Optional**, so a person can use `patch` by hand. The skill always passes it |
| P5 | Should a successful patch also run `compile`? | **No.** One command, one effect. The skill's loop already runs `compile` next |

## Tests

- **One test per edit kind and per way it can fail** (path missing, parent not a map, rename onto an existing name), asserting the answer and that nothing was written.
- **Golden files for the text edits:** a commented, block-style file and a flow-style file, each through `set` (new key, existing value), `remove` (with its comment lines) and `rename`, compared byte for byte.
- **Atomicity:** a call with one bad edit among three leaves the file byte-identical.
- **Conflict:** a wrong `--expected-revision` exits `3`, writes nothing, and reports both revisions. The right one applies.
- **The result is validated as a whole:** an edit that makes the design invalid reports the diagnostic at its position in the would-be file, and writes nothing. A `--dry-run` writes nothing on a valid patch too.
- **The classification matches `diff`:** for a patch and for the same change made by hand, the two answers list the same changes.
- **A round trip:** `patch` then `plan` shows the rename as a rename (the `was` key is there).
- **CLI tests run the real binary,** and an MCP test calls the `patch` tool.
