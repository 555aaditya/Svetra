# Rules for the new row: `check` on update, and restrictive policies (draft)

This document specifies two additions to policies, so that a rule can say what an update is allowed to turn a row **into**, and so that one rule can narrow others instead of only adding to them. When the code and this document disagree, that's a bug in one of them.

**Status: built in 0.8 (unreleased).** The open decisions took the recommendations: follow PostgreSQL and warn (`SVT034`), the names `check` and `restrictive`, restrictive policies worded as a limit on the sentences they narrow, no `check` on insert, column rules in their own spec, and no cap on the generated transition checks. What shipped differs from the draft in two places, both found while building: the test world's **second `user` field now points at the other person** (so an author and a reviewer are two different people), and the warning found a real flaw in the first version of the approvals example (a reviewer could make themselves the author), fixed with a constraint.

**Done when:** the document-approval app from the agent evaluation (`eval/agent/prompts/approvals.md`) can be written in `svetra.yaml` with no holes for its status changes, and the generated access tests prove it.

## Why

A policy has one condition, `when`, and it plays two roles at once in an update: the filter on the row **as it is now** (`USING`), and the check on the row **as the update leaves it** (`WITH CHECK`). Today they are the same expression. That cannot say:

- *"An author may submit a draft"*: the row must be a draft before and a draft **or submitted** after. One expression can't be both.
- *"A reviewer may approve or reject a submitted document"*: submitted before, approved or rejected after.
- *"Nobody can change a document once it's approved"*: a rule for what must hold afterwards, separate from who may start.

In the agent evaluation, an agent given exactly this application wrote **four hand-written holes** to get status transitions, because the format couldn't express them, and one of those hole bodies had a syntax error that stopped the whole migration. Holes are the escape hatch; a state change on a row is too common to need one.

Separately, every policy today is **permissive**: they combine with `or`, so a new policy can only ever widen access. "Suspended people can't write anything" or "deleted rows are hidden from everyone" can only be written by editing every other policy. PostgreSQL has **restrictive** policies for this: all of them must hold, in addition to at least one permissive policy.

## 1. `check` on a policy

```yaml
policies:
  authors_edit_drafts:
    on: Doc
    allow: [update]
    when:  "author == auth.user and status == 'draft'"            # the row as it is now (USING)
    check: "author == auth.user and status in ['draft', 'submitted']"  # the row after (WITH CHECK)
```

| Key | Meaning |
|---|---|
| `when` | As today: the condition on the row **before** the action, for `select`, `update` and `delete`; and for `insert`, the row being created |
| `check` (new, optional) | The condition the row must satisfy **after** an `update`. When absent it is `when`, so every existing file means exactly what it meant |

Rules:
- **`check` is for `update` only.** A policy that allows `select`, `delete` or `insert` and has `check` is `SVT033`, because those actions have no "after" different from "before". A policy that allows `update` along with other actions may have `check`; it applies to the update only.
- **`check` is typed like `when`** (a boolean over the entity's own fields, one-hop paths, `auth.user` and `has_role`), in the entity's scope. A type error is `SVT003`.
- **It compiles** to `WITH CHECK (<check>)` on the update policy, and `USING (<when>)` as today.
- **An update must satisfy a policy's `when` on the old row and its `check` on the new row, and both from the same policy.** Permissive policies combine with `or` as a whole: one update policy that fits is enough. (This is PostgreSQL's behaviour, checked on PostgreSQL 16: with policy A `using (x = 1) with check (x = 1)` and policy B `using (x = 2) with check (x = 3)`, updating a row with `x = 1` to `x = 3` is **allowed**, because the old row passes A's `USING` and the new row passes B's `WITH CHECK`. The evaluator follows PostgreSQL, not the intuition. See Open **U1**.)

## 2. Restrictive policies

```yaml
policies:
  suspended_cant_write:
    on: Post
    allow: [insert, update, delete]
    restrictive: true
    when: "not suspended"
```

| Key | Meaning |
|---|---|
| `restrictive` (new, optional, default `false`) | `true`: the condition must hold **in addition** to at least one permissive policy that allows the action. It never grants access by itself |

Rules:
- **A restrictive policy only narrows.** An entity with only restrictive policies has no permissive one for any action, so nobody can do anything: `SVT001` still applies (the entity has no rule that lets anyone in), with a hint that restrictive policies need a permissive one to restrict.
- **All restrictive policies for an action must hold,** each on its own `when` (and `check` for updates), for the person's database role.
- **It compiles** to `create policy ... as restrictive`.
- **`explain`** words it as a limit: "…and only when the author isn't suspended." (Open **U3**.)

## 3. The evaluator and the access tests

The access tests' oracle must follow PostgreSQL exactly, because an independent oracle that disagrees is a false alarm or a false pass. For an `update` of row R to row R′ by a person P (docs/compiler.md, Access tests), expected **allowed** when all hold:

1. Some permissive policy allowing `update` for P has `when` true on **R**.
2. Every restrictive policy allowing `update` has `when` true on **R**.
3. Some permissive policy allowing `update` has `check` true on **R′** (its `when` when `check` is absent). Every restrictive policy allowing `update` has `check` true on **R′**.
4. The same as 1 to 3 for `select`, on R and on R′, because the generated tests update with a `where` clause, which reads the row, and PostgreSQL then applies the select policies to both rows.

(For the single-policy reading in the Open decision **U1**, 1 and 3 must come from the *same* policy.)

### New checks: changing a field to each value a rule mentions

Today an update check sets a row's primary key to itself (R′ = R) and the "change who owns a row" checks swap one `user` or `ref` field. With `check`, the interesting updates are the ones that change a field the **`check`** talks about. The generator therefore adds, for each update policy with a `check`, and each field its `check` or `when` reads:

| Field type | New values tried |
|---|---|
| `enum` | each declared value |
| `bool` | `true` and `false` |
| `user` | alice and bob (as today) |
| `ref` | the target's other row (as today) |
| anything else | not tried (a value that isn't one of a small set can't be separated from another by a rule) |

Each is one check per person and stored row: "alice can set Doc a's status to submitted", "alice can't set Doc a's status to approved". The expected result comes from the rules above with R′ = R with that one field changed. Roughly 2 to 4 checks per such field and row per person. The "change who owns" checks become the `user` and `ref` rows of this table.

`svetra view` gains no new column: **Change** stays one cell, but its detail panel lists the transitions: "an author can change a draft to draft or submitted; a reviewer can change a submitted document to approved or rejected."

## 4. `svetra plan`

- Adding a restrictive policy, or tightening a `check`, **narrows** access: reported, never blocking.
- Removing a restrictive policy, or loosening a `check`, **widens** access, like any other wider change: needs `--accept wider_access`.
- Splitting a `when` into `when` and `check` changes nothing until the two differ; the access diff (computed from the oracle) tells the truth either way.

## 5. What this does not do

**A reviewer who approves must not also change the title.** In the approvals prompt that is a rule about **columns**: the update may change `status` and nothing else. A `check` on the new row can't say it, because the new row alone doesn't show what changed. It needs the old row beside the new one, which row-level security doesn't offer, so PostgreSQL implementations use a trigger. This document does not add it. It is the next item, written up in [column-rules.md](column-rules.md): `only: [status]` on an update policy, compiled to a generated guard trigger and proven by the access tests. It is tracked as Open **U5**, and the agent evaluation showed that 4 of 4 agents needed it. Until then, an application that needs it uses a hole, as the agent evaluation's agents did, and the access tests will say so: the check "the reviewer cannot approve and retitle at once" fails on an app written without the trigger.

**Row-level security can't count or look across rows** (`quota`, "at most 3 open documents"). Holes and triggers remain the way.

## 6. The approvals example, written with this

```yaml
entities:
  Doc:
    fields:
      id:       { type: uuid, primary: true }
      author:   { type: user, required: true }
      reviewer: { type: user, required: true }
      title:    { type: text, required: true }
      status:   { type: enum, values: [draft, submitted, approved, rejected], default: draft, required: true }
policies:
  authors_see:    { on: Doc, allow: [select], when: "author == auth.user" }
  reviewers_see:  { on: Doc, allow: [select], when: "reviewer == auth.user and status != 'draft'" }
  authors_start:  { on: Doc, allow: [insert], when: "author == auth.user and status == 'draft'" }
  authors_edit:   { on: Doc, allow: [update], when: "author == auth.user and status == 'draft'",
                    check: "author == auth.user and status in ['draft', 'submitted']" }
  reviewers_decide:
    on: Doc
    allow: [update]
    when:  "reviewer == auth.user and status == 'submitted'"
    check: "reviewer == auth.user and status in ['approved', 'rejected']"
  authors_delete_drafts: { on: Doc, allow: [delete], when: "author == auth.user and status == 'draft'" }
```

This passes every check of the approvals oracle except the one in section 5 ("approve and retitle at once"), which needs column rules, and no hole is written.

## Open decisions

| # | Question | Recommendation |
|---|---|---|
| **U1** | PostgreSQL lets `USING` come from one permissive update policy and `WITH CHECK` from another. Should Svetra instead require both from the **same** policy, which is how a person reads one rule? | **Follow PostgreSQL in the evaluator, and warn** (new `SVT034`, warning) when two update policies' `when` and `check` could combine into a transition neither intended. The warning is computed by the same evaluator. Silent divergence from PostgreSQL would make the access tests lie |
| **U2** | The key names: `check`, and `restrictive: true` | `check` (matches SQL and the generated tests' words) and `restrictive: true` (matches PostgreSQL, searchable). Alternatives: `after:` and `deny:` |
| **U3** | How `explain` words a restrictive policy | A trailing limit on every sentence it narrows: "…, and only when the author isn't suspended." The reader sees the effect where it applies, not a rule far away |
| **U4** | Should `insert` get a `check` too (for the inserted row, separate from `when`)? | **No.** For insert, `when` already *is* the check on the new row. Revisit only if a use appears |
| **U5** | Column rules (`unchanged: [title, author]`): this document, or its own? | **Its own spec.** It compiles to a generated trigger, which is a different mechanism with its own proof obligations (the trigger must agree with the policy that granted the update) |
| **U6** | The new transition checks multiply the test count. Cap them? | No cap on the generated count, but only fields a rule reads are tried, so an app without `check` generates **no** new checks (all goldens stay byte-identical) |

## Build order, once approved

1. **Spec and diagnostics:** `check` and `restrictive` in the loader, JSON Schema and `docs/format.md`; `SVT033`; the typeck.
2. **Compile:** the update policy's `WITH CHECK`; `as restrictive`.
3. **The evaluator** exactly as in section 3, and the transition checks, with a new example (`approvals`) added to the fixtures, its golden files and `pg-check`/`supabase-check`.
4. **`explain`, `view`, `context`, `graph`, `plan`.**
5. **The agent evaluation re-run on `approvals`:** how many holes does the agent write now?

## Tests

- **Every existing golden file unchanged** (no `check`, no `restrictive`).
- **The approvals example passes its generated access tests on a real Supabase**, and each deliberate breakage (the `check` dropped; a restrictive policy removed) fails exactly the expected checks.
- **The evaluator against PostgreSQL:** for random policy pairs with `when` and `check` that differ, the expected result equals what PostgreSQL does (this is also the first use of the differential-testing idea in the research notes).
