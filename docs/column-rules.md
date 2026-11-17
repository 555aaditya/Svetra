# Column rules: `only` on an update policy (draft)

This document specifies a rule for which **fields** an update may change. When the code and this document disagree, that's a bug in one of them.

**Status: built in 0.9 (unreleased).** The open decisions took the recommendations: the allow-list `only` (C1), a field is free when some applicable permissive policy frees it (C2), the service role, owner and hole bodies are exempt (C3), `only` on restrictive policies with the opposite rule (C4), the guard in the generated schema (C5), `immutable` later and separately (C6), and no guard on insert (C7).

**Done when:** the approvals app from the agent evaluation (`eval/agent/prompts/approvals.md`) is written entirely in `svetra.yaml`, with no hole and no hand-written SQL, passes every one of its 24 oracle checks, and the generated access tests prove the column rule on the database.

## Why

`check` on update (docs/update-check.md) lets a rule say what an update may turn a row **into**: a draft may become submitted, a submitted document approved or rejected. It cannot say **what the update may touch**. In the approvals prompt a reviewer may approve or reject a document but "cannot change anything else about it", so a reviewer must not be able to approve **and** retitle it in one update. A condition on the new row can't tell: the new row alone doesn't show what changed.

The agent evaluation measured how much this matters. After `check` was built (Svetra PR #69), four runs of the approvals prompt with Svetra all used `check` for the status changes and wrote **no holes**, and **all four** then appended a hand-written trigger to their migration for this one rule. The result was correct (24 of 24 checks), but the trigger lives outside `svetra.yaml`: it isn't in the design, `explain` doesn't mention it, and the generated access tests don't cover it. That is exactly the kind of rule Svetra exists to keep out of hand-written SQL.

Row-level security cannot do this by itself. A policy sees one row at a time, the old one in `USING` and the new one in `WITH CHECK`, never both. Comparing old and new needs a row trigger, which is what the agents wrote and what this feature would generate and test.

## 1. `only` on a policy

```yaml
policies:
  reviewers_decide:
    on: Doc
    allow: [update]
    when:  "reviewer == auth.user and status == 'submitted'"
    check: "reviewer == auth.user and status in ['approved', 'rejected']"
    only: [status]
```

| Key | Meaning |
|---|---|
| `only` (new, optional) | The fields an update **allowed through this policy** may change. Every other field must be left as it is. Names fields of the policy's entity, at least one. Only for a policy that allows `update` (`SVT035` otherwise). Without `only`, the policy frees every field, as today |

- **An allow-list, on purpose: it fails closed.** Add a column to `Doc` later and the reviewer still can't touch it, because it isn't in their `only`. A list of fields that *can't* change would silently leave a new column open, which is a leak by omission; a security tool should be closed by default, as everything else in Svetra is. (Open **C1** discusses the alternative.)
- **"Changed" means `is distinct from`.** An update that sets a field to the value it already has isn't a change, so a client that sends the whole row back still works. A null that stays null isn't a change either.
- **It never grants anything.** `only` only refuses updates that would otherwise be allowed. A policy with `only` and a `when` that never holds restricts nothing.
- **The primary key** can't be listed: it is never changed through the API, and the checks identify rows by it (`SVT002`).
- **Insert and delete are unaffected.** A new row has no old values; a deleted row has no new ones.

## 2. When several policies apply to the same update

A person can be covered by more than one update policy on the same row: an author who is also a reviewer, or an admin policy beside a member policy. Permissive policies combine with `or`, so the rule for fields follows the same idea:

> **A field is locked for an update by person P on row R when no permissive update policy that applies to P on R leaves it free.**
>
> A permissive policy **applies** to P on R when its `when` holds on R (the old row) for P's database role. It **leaves a field free** when it has no `only`, or the field is in its `only`.

So `only` on one policy cannot take away something another applicable policy grants: an author's own policy, with no `only`, still lets them retitle, even when the same person also holds a reviewer's policy that allows only the status. This mirrors how a grant works everywhere else in Svetra.

**Restrictive policies** (docs/update-check.md) may also have `only`, and for them it is the other way round: **a field is locked if any applicable restrictive policy omits it from its `only`**, whatever the permissive ones say. A restrictive policy exists to narrow, so "while an order is paid, only its notes may change" is one restrictive rule:

```yaml
  paid_orders_are_final:
    on: Order
    allow: [update]
    restrictive: true
    when: "status == 'paid'"
    only: [notes]
```

Putting it together, an update of row R by person P is refused when any field it changes is locked as above; otherwise the ordinary rules (`when`, `check`, restrictive policies, select policies) decide, exactly as before.

**Example, on the approvals app.** Alice wrote document D and bob reviews it, and D is submitted.

| Update by bob | Result | Why |
|---|---|---|
| status `submitted` → `approved` | allowed | `reviewers_decide` applies, `status` is in its `only`, and the `check` holds |
| title changed, status unchanged | refused | the `check` fails (status must become approved or rejected); the column rule isn't even reached |
| status → `approved` **and** title changed | **refused** | the `check` holds, but `title` isn't in `reviewers_decide`'s `only`, and no other policy applies to bob on D that leaves it free. This is the case no row policy can express |
| status → `approved`, title "changed" to the same text | allowed | not a change |

## 3. How it compiles

One `before update` row trigger per entity that has any `only`, and one function behind it, both in the generated schema file (so they come with `supabase db diff`, are checked by `compile --check`, and are owned by Svetra):

```sql
create function "svetra"."guard_doc"() returns trigger language plpgsql as $$
begin
  -- Only people calling through the API are checked: the same roles row-level security applies to.
  if current_user not in ('anon', 'authenticated') then return new; end if;
  if new."title" is distinct from old."title" and <title is locked> then
    raise exception 'doc: title can''t be changed here' using errcode = '42501';
  end if;
  -- ... one block per non-key field that some policy's `only` leaves out
  return new;
end $$;
create trigger "svetra_guard" before update on "public"."doc"
  for each row execute function "svetra"."guard_doc"();
```

`<title is locked>` is built from the policies, exactly as section 2 says: each applicable policy is its `when` evaluated on **`old`**, for the person's database role.

- **Restrictive lock:** `<any applicable restrictive policy has an `only` that leaves title out>`.
- **Permissive lock:** at least one permissive update policy applies and **none** that applies leaves `title` free. In SQL, `not (<policy A applies and has no only, or lists title> or <policy B applies and …> or …)`.
- **"Applies"** is the policy's `when` compiled against `old`: columns become `old."col"`, a one-hop path becomes the same scalar subquery as in a policy keyed on `old."fk"`, `auth.user` is `(select auth.uid())`, and `has_role(...)` is `svetra.has_role(...)`. A policy `as anon` only applies when `current_user = 'anon'`, one `as authenticated` when it is `'authenticated'`, one `as anyone` to both.
- **The refusal is SQLSTATE 42501** (insufficient privilege), like any other refusal by a policy, so a client can't tell it apart, and the generated tests count it as denied.

**Order of events in PostgreSQL:** `USING` filters the rows an update can see, then `before` triggers run, then `WITH CHECK` runs on the result. So the trigger only ever sees rows the person could already update, and it never changes `new`: it only refuses. If no policy applies, the update is already refused before the trigger.

**Checked on PostgreSQL 16** (a table with `update` policy `using (id = 1) with check (x = 99)` and a `before update` trigger that raises a notice): an update of the row with `id = 1` ran the trigger and then failed `WITH CHECK`; an update of the row with `id = 2` ran no trigger, because `USING` hid it; and `current_user` inside the trigger was the calling role, not the table owner. The guard relies on exactly these three facts.

**What is exempt, on purpose:**
- **The service role, the table owner and migrations.** They aren't `anon` or `authenticated`, so row-level security doesn't apply to them, and neither does this.
- **Hole bodies.** A hole runs as its own role (docs/compiler.md), which isn't either of those, so it is exempt as it is from the policies. That is what makes a hole the right place for logic that must change a locked field, such as an administrator's tool.

**Cost:** one extra function call per updated row, and only for entities that use `only`. The expressions are the policies' own, on a row already in memory.

## 4. The access tests

The expected results come from the evaluator, which reads the policies, never the generated trigger. For an entity with any `only`, the generator adds checks of three kinds, for every person and stored row (`a` and `b`). "Locked" is computed from the policies exactly as in section 2, on the old row, for that person.

| Check | Update | Expected |
|---|---|---|
| **A field on its own** | one field set to the other row's value (every stored row differs in every field, so it is a real change), for every non-key field | allowed when the ordinary rules allow that update **and** the field isn't locked; otherwise denied |
| **A locked field beside an allowed transition** | for each field F that is locked for this person and row, and each transition G → v that the ordinary rules allow for this person and row (docs/update-check.md, section 3): G set to v **and** F changed, in one update | **denied** whenever F is locked. This is the check no row policy could pass, and the one that proves the trigger exists and works |
| **A free field beside an allowed transition** | the same, with a field that isn't locked | as the ordinary rules say for the combined row |

- **Why the second row is the point.** A locked field changed *alone* is usually refused by `WITH CHECK` anyway (the status didn't move, so the `check` fails), so it can't tell a working trigger from none. The combined update is allowed by every row policy and is refused only by the guard. A design where the trigger is missing, disabled or wrong fails exactly those checks.
- **Several changed fields** need the evaluator's "new row" to hold more than one changed field; it holds a list of them, where today it holds one.
- **Skipped, as for the existing change checks:** fields alone in a `unique` rule, and fields a declared constraint names, because the database could refuse the new value for another reason.
- **Entities with no `only` get none of these checks,** so every existing golden file stays byte-identical.
- **Roughly** 3 to 5 extra checks per person per row per field of an entity that uses `only`. The approvals example is a few dozen.

**Deliberate breakages for the real-database checks** (`tests/pg/breakages/`, the same mechanism as for policies): the trigger dropped; the trigger disabled (`alter table ... disable trigger`); a field removed from the guard by hand; and the restrictive lock ignored. Each must fail exactly the checks above and nothing else.

## 5. `explain`, `view`, `context`, `graph` and `plan`

- **`explain`** adds a sentence to a policy that has `only`: *"…and they can change only its status."* For a restrictive one: *"…while it is paid, only its notes can change."* The words are the fields' own labels. The generated trigger is never mentioned, because the reader needs the rule, not the mechanism.
- **`context`** appends `only status` to the policy's line.
- **`graph`** is unchanged: a column rule grants nothing and draws no edge.
- **`view`** has no new column: **Change** stays one cell per person and thing. Its detail panel lists, for a person and row, what the change may touch: "a reviewer can change a submitted document's status, to approved or rejected, and nothing else". (This is also where docs/update-check.md's listing of allowed transitions lands.)
- **`plan`** (docs/change.md) classifies by the access diff it already computes from the evaluator. Adding an `only`, or dropping a field from one, **narrows** access; removing one, or adding a field to one, **widens** it and needs `--accept wider_access`. Adding the first `only` to an entity adds the guard trigger, which is a schema addition and safe for data; removing the last one drops it. A column added later is locked by every `only` that doesn't list it: the access diff says so, as a narrower access for the people those policies cover.

## 6. Diagnostics

| Code | Severity | Rule |
|---|---|---|
| `SVT035` (new) | error | A policy has `only` but doesn't allow `update`: only an update changes fields |
| `SVT002` (existing) | error | `only` names a field the entity doesn't have, or its primary key |
| `SVT029` (existing) | error | `only` lists no field: an update allowed through the policy could change nothing, which is never what is meant |
| `SVT036` (new) | warning | A person is covered, on some row, by two permissive update policies that disagree about a field: one has an `only` that leaves it out and another frees it (no `only`, or it is listed). By section 2 the second one frees it, so the first one's limit has no effect for that person. Computed by the evaluator, like `SVT034` |

`SVT036` is the column-rule twin of `SVT034`: a combination of policies that does something neither says alone. The hint is to give the freeing policy an `only` too, or to merge the two.

## 7. Alternatives considered

- **Column privileges** (`grant update (status) on doc to authenticated`). They are per database role, not per row or state, so they can't say "only while the document is submitted", and the approvals rule needs exactly that. They are the lighter tool for a field **nobody** may change after creation, such as `author` or `created_at`. That is a different, simpler feature, a field flag `immutable: true` (Open **C6**), and it could share the same tests.
- **A `check` constraint.** It sees one row, so it can't compare old and new.
- **A view with `instead of` triggers.** Heavier, and the API would have to be pointed at the view.
- **A list of fields that can't change (`unchanged`)** is the same power, written the other way round, and the first draft of this document used it. It fails open: a column added later is free for everyone the rule was meant to limit. The approvals prompt itself is an allow-list ("cannot change anything else"). Open **C1**.
- **Leave it to holes and hand-written triggers.** This is what happens today, and what the agent evaluation measured: four of four agents wrote the trigger by hand, outside the design, where it isn't explained, tested or diffed. A rule Svetra can't see is a rule Svetra can't prove.
- **Check it in the app.** Not enforceable: the browser talks to the database directly.

## 8. The approvals app, complete

```yaml
  reviewers_decide:
    on: Doc
    allow: [update]
    when:  "reviewer == auth.user and status == 'submitted'"
    check: "reviewer == auth.user and status in ['approved', 'rejected']"
    only: [status]
```

With this single added line, the whole prompt is in `svetra.yaml`: the author's rules, the reviewer's, the transitions, and the column rule. No hole, no hand-written SQL, and every one of the 24 oracle checks is expected to pass, including "the reviewer cannot approve and retitle at once". Add a `notes` column to `Doc` next month and the reviewer still can't edit it, with no change to the rule.

## Open decisions

| # | Question | Recommendation |
|---|---|---|
| **C1** | An allow-list (`only: [status]`) or a deny-list (`unchanged: [title, author]`)? | **`only`.** It fails closed: a column added later stays locked for the people the rule limits. The deny-list is the same power but leaves new columns open, a leak by omission. Naming: `only` reads as the prompt does ("can change only the status"); alternatives `changes:` or `may_change:` |
| **C2** | How policies combine: a field is free when **some** applicable permissive policy frees it, or locked when **any** one locks it? | **Some frees it** (section 2). It follows the "permissive policies add up" meaning that everything else in Svetra has, and the warning `SVT036` catches the surprising case. The other reading makes a policy able to take away what another grants |
| **C3** | The service role, the table owner and hole bodies are exempt (the trigger only checks `anon` and `authenticated`) | **Yes, and say so in `explain`'s "before going live".** They bypass row-level security too, so a rule that bound them but not policies would be inconsistent. A hole is the right place for an administrator's tool |
| **C4** | Should `only` be allowed on restrictive policies? | **Yes**, with the opposite combining rule (any applicable restrictive policy that omits a field locks it). It is the natural way to say "while paid, only notes change" |
| **C5** | Where do the generated trigger and function live? | In a `svetra` schema next to `svetra.has_role` (the API doesn't expose it), named `guard_<table>` and `svetra_guard`, in the generated schema file, so `compile --check` and `supabase db diff` treat them like everything else Svetra writes |
| **C6** | Also add a field flag `immutable: true` (never changes after creation, for everyone), as sugar compiled to column privileges or to the same guard? | **Later, separately.** It is simpler than `only`, but the approvals rule is the evidence for this one, and the tests would be shared |
| **C7** | Should the guard also protect insert, by refusing an insert that sets a field the policy doesn't allow? | **No.** A column rule is about changing what is there. For insert, `when` already is the check on the new row |

## Build order, once approved

1. **Format:** `only` in the loader, JSON Schema and `docs/format.md`; `SVT035`, `SVT002` and `SVT029` for the bad cases; the model and `plan`'s comparison.
2. **Compile:** the guard function and trigger, with the old-row expression compiler (`old."col"`).
3. **The evaluator:** lock computation from section 2, a `Swap` that holds several changed fields, and the three kinds of check in section 4, with `approvals` gaining the line in section 8 (its goldens change, which is the point).
4. **`SVT036`,** computed by the same evaluator.
5. **`explain`, `context`, `view` and `plan`.**
6. **Real databases:** `pg-check.sh` and `supabase-check.sh` run the approvals app with its breakages (trigger dropped, trigger disabled, a field removed from the guard, restrictive lock ignored).
7. **The agent evaluation again** on the approvals prompt: how many agents still write a trigger by hand?

## Tests

- **Every existing golden file unchanged** (no `only` anywhere).
- **The approvals app passes all 24 oracle checks of the agent evaluation** when built from `svetra.yaml` alone, on plain PostgreSQL 16 and on a real Supabase.
- **Each deliberate breakage fails exactly the checks it should,** and `svetra check` finds nothing critical.
- **The evaluator against PostgreSQL,** for random policy sets that include `only`: the expected result equals what the database does (extending the differential-testing idea in the research notes).
