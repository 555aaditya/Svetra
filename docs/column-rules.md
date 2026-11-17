# Column rules: `unchanged` on an update policy (draft)

This document specifies a rule for which **fields** an update may change. When the code and this document disagree, that's a bug in one of them.

**Status: draft for review.** The decisions marked **Open** need the maintainer's answer before building. Nothing here is built.

**Done when:** the approvals app from the agent evaluation (`eval/agent/prompts/approvals.md`) is written entirely in `svetra.yaml`, with no hole and no hand-written SQL, passes every one of its 24 oracle checks, and the generated access tests prove the column rule on the database.

## Why

`check` on update (docs/update-check.md) lets a rule say what an update may turn a row **into**: a draft may become submitted, a submitted document approved or rejected. It cannot say **what the update may touch**. In the approvals prompt a reviewer may approve or reject a document but "cannot change anything else about it", so a reviewer must not be able to approve **and** retitle it in one update. A condition on the new row can't tell: the new row alone doesn't show what changed.

The agent evaluation measured how much this matters. After `check` was built (Svetra PR #69), four runs of the approvals prompt with Svetra all used `check` for the status changes and wrote **no holes**, and **all four** then appended a hand-written trigger to their migration for this one rule. The result was correct (24 of 24 checks), but the trigger lives outside `svetra.yaml`: it isn't in the design, `explain` doesn't mention it, and the generated access tests don't cover it. That is exactly the kind of rule Svetra exists to keep out of hand-written SQL.

Row-level security cannot do this by itself. A policy sees one row at a time, the old one in `USING` and the new one in `WITH CHECK`, never both. Comparing old and new needs a row trigger, which is what the agents wrote and what this feature would generate and test.

## 1. `unchanged` on a policy

```yaml
policies:
  reviewers_decide:
    on: Doc
    allow: [update]
    when:  "reviewer == auth.user and status == 'submitted'"
    check: "reviewer == auth.user and status in ['approved', 'rejected']"
    unchanged: [title, author, reviewer]
```

| Key | Meaning |
|---|---|
| `unchanged` (new, optional) | Fields an update **allowed through this policy** may not change. Names fields of the policy's entity. Only for a policy that allows `update` (`SVT035` otherwise) |

- **"Changed" means `is distinct from`.** An update that sets a field to the value it already has isn't a change, so a client that sends the whole row back still works. A null that stays null isn't a change either.
- **It never grants anything.** `unchanged` only refuses updates that would otherwise be allowed. A policy with `unchanged` and a `when` that never holds locks nothing.
- **The primary key** can't be listed: it is never changed through the API, and the checks identify rows by it (`SVT002`).
- **Insert and delete are unaffected.** A new row has no old values; a deleted row has no new ones.

## 2. When several policies apply to the same update

A person can be covered by more than one update policy on the same row: an author who is also a reviewer, or an admin policy beside a member policy. Permissive policies combine with `or`, so the rule for fields must follow the same idea:

> **A field is locked for an update by person P on row R when no permissive update policy that applies to P on R leaves it free.**
>
> A permissive policy **applies** to P on R when its `when` holds on R (the old row) for P's database role. It **leaves a field free** when the field isn't in its `unchanged`.

So `unchanged` on one policy cannot take away something another applicable policy grants: an author's own policy that lets them retitle keeps working even when the same person also holds a reviewer's policy that locks the title. This mirrors how a grant works everywhere else in Svetra.

**Restrictive policies** (docs/update-check.md) may also have `unchanged`, and for them it is the other way round: **a field is locked if any applicable restrictive policy lists it**, whatever the permissive ones say. A restrictive policy exists to narrow, so "while an order is paid, nobody changes its total" is one restrictive rule:

```yaml
  paid_orders_are_final:
    on: Order
    allow: [update]
    restrictive: true
    when: "status == 'paid'"
    unchanged: [total, customer]
```

Putting it together, an update of row R by person P is refused when any field it changes is locked as above; otherwise the ordinary rules (`when`, `check`, restrictive policies, select policies) decide, exactly as before.

**Example, on the approvals app.** Alice wrote document D and bob reviews it, and D is submitted.

| Update by bob | Result | Why |
|---|---|---|
| status `submitted` → `approved` | allowed | `reviewers_decide` applies, the status isn't in its `unchanged`, and the `check` holds |
| title changed, status unchanged | refused | the `check` fails (status must become approved or rejected); the column rule isn't even reached |
| status → `approved` **and** title changed | **refused** | the `check` holds, but `title` is in `reviewers_decide`'s `unchanged`, and no other policy applies to bob on D that leaves it free. This is the case no row policy can express |
| status → `approved`, title "changed" to the same text | allowed | not a change |
