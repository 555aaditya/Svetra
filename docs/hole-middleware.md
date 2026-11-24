# Hole middleware, proposal for version 0.2

This document specifies the checks Svetra can put in front of a `function` hole so that the agent doesn't write them by hand in every body. It is the second half of roadmap milestone M4d. The first half, `audit: true`, is built and lives in [format.md](format.md#audit). This is a proposal: nothing here is built yet, and once it is, [`format.md`](format.md) and [`compiler.md`](compiler.md) become the authority.

A hole body is the one hand-written part of a Svetra app, so it is where an agent's mistakes concentrate. Three things are written again in almost every body: **refuse bad input**, **don't do the same thing twice when the client retries**, and **say who called**. The third is `audit`. This document is about the first two, and says why a fourth, rate limits, is left out.

**Done when:** see [Done when](#done-when) at the end.

## What stays the same

- Who may call a hole is still decided by `as` and `when`, and a refused call is still SQLSTATE `42501`.
- The body is still statements only, in `supabase/holes/<name>.sql`, and runs as the hole's own role.
- Nothing here needs a new schema the API can reach, a new dependency or a clock. Both features are plain SQL in the two generated functions.
- Deterministic output: the same file always compiles to the same bytes.

## 1. Input validation: `require`

### The gap

`when` is for **who**. It runs first and refuses with `42501`, which every client reads as "not allowed". Putting "the amount is positive" in `when` works, but it tells a person "not allowed" when the truth is "that amount is wrong", and it mixes two questions that `explain` should answer separately. Today the agent writes `if p_amount <= 0 then raise exception ...; end if;` at the top of the body, in a different wording each time.

### The key

A hole gets `require`, a list of expressions over its inputs:

```yaml
holes:
  transfer:
    kind: function
    input:
      source: Account.id
      destination: Account.id
      amount: Transaction.amount
    when: "source.owner == auth.user"
    require:
      - "amount > 0"
      - "source != destination"
```

- **The language is the one `when` uses.** Inputs are written without `p_`, `Entity.<primary key>` inputs allow one hop, `auth.user` and `has_role(...)` work. Each expression must be boolean (`SVT003` otherwise, as for `when`). No new syntax and no new diagnostic code.
- **Order: `when` first, then `require`, then the body.** Who may call is settled before anyone learns what the rules for input are, so a stranger can't probe the validation by calling.
- **A null is a failure.** SQL's `check` lets a null pass. A validation must not: `amount > 0` with a missing amount is refused. This is the same rule `when` already has (`coalesce(..., false)`).
- **The refusal is SQLSTATE `22023` (`invalid_parameter_value`)** with the message `<hole>: <the expression as written>`, for example `transfer: amount > 0`. The first failing expression is reported, in list order. Clients and agents can tell it from `42501`, and the message says which rule failed without leaking a row.
- **It runs with the hole's access,** like `when`, so reading through an input into an entity the hole doesn't declare in `reads` or `writes` is `SVT003`.

### What it compiles to

In the body function, directly after the `when` guard and before the body, one statement per expression:

```sql
if not coalesce(<expr>, false) then
  raise exception 'transfer: amount > 0' using errcode = '22023';
end if;
```

Nothing changes in the public function, and a hole with no `require` compiles to the same bytes as today.

### Not mirrored in TypeScript

The generated Zod schema for a hole's inputs is unchanged: the format has no `.refine` yet, and the database is the authority. A client that skips Zod gets the same answer from the database. Mirroring simple comparisons in Zod is a follow-up, and the same follow-up mirrors the entity constraints (whitepaper W2.3.2).

### Open question: inherit the field's constraints?

An input typed `Transaction.amount` already gets the field's **type**. It could also get every entity constraint that reads only that field (`amount_positive: "amount > 0"`), so `require` would only be needed for rules across inputs. That makes "a constraint is stated once and holds everywhere" true for holes too. It is not in this proposal because it needs a rule for which constraints count as "only that field", and it changes the behaviour of every existing hole with a typed input. **Decision waiting on the owner.**

## 2. Retries: `idempotent`

### The gap

Mobile clients retry, and so do browsers, proxies and agents. A `transfer` called twice moves the money twice. The usual fix is a client-chosen key that makes the second call a no-op, and agents write it by hand (a table, an insert, an `on conflict`) with the same slips: the key isn't scoped to the caller, the second call returns an error instead of the first answer, or the same key is accepted for different inputs.

### The key

```yaml
holes:
  transfer:
    input:
      request: uuid
      source: Account.id
      ...
    idempotent: request
```

- **`idempotent` names one input,** which is the key. It must be typed `uuid` or `text`.
- **The key is the caller's own.** Records are kept per (hole, caller, key), so one person's key never matches another's and nobody can learn whether a key exists. Because a logged-out visitor has no identity to scope by, a hole with `idempotent` can't have `as: anon` or `as: anyone` (**`SVT041`**, error).
- **A repeated call returns the first call's answer** and runs nothing: not the guard's side effects, not the body, not the audit record (the first call's record is the record).
- **The same key with different inputs is refused,** SQLSTATE `22023`, message `<hole>: request was already used with different inputs`. Comparing the inputs, not only the key, stops a buggy client from silently getting the wrong answer.
- **A failed call leaves no key.** An exception rolls the key back with the rest, so the client's retry runs normally.
- **The guard runs on a replay too** (see below), so a person whose role was removed since the first call can't read a stored answer.
- **The answer is stored as `jsonb`:** `to_jsonb` of what the body returned, and `null` for a hole with no `output`. A replay converts it back to the hole's declared return type.

### What it compiles to

A table, created only when some hole has `idempotent`, in the schema the API doesn't expose, with no grants and row-level security with no policy (the same stance as the audit log):

```sql
create table "svetra"."hole_keys" (
  "hole"   text   not null,
  "caller" uuid   not null,
  "key"    text   not null,
  "inputs" jsonb  not null,
  "result" jsonb,
  primary key ("hole", "caller", "key")
);
```

It is written by the public function, which is owned by the migration's role, so the hole's own role never touches it, and the body can't forge or erase a key. The public function becomes: take the key and inputs; look up the row; on a hit with equal inputs, run the guard and return `result`; on a hit with different inputs, raise `22023`; on a miss, call the body and insert the row. Two calls racing on one key are settled by the primary key: the loser's insert fails, so its work rolls back, and its retry gets the winner's answer.

**One extra function.** The guard has to run on a replay, but it lives in the body function, whose body must not run. When a hole has both `idempotent` and `when`, the guard is generated once as `svetra_holes.<name>_allowed(p_caller, <inputs>) returns boolean`, executable only by the migration's role, and called from both places. A hole with no `idempotent` compiles to the same bytes as today.

### Retention is not in this version

The table grows by one row per call. Pruning, and a time window after which a key may be reused, are left out until real use shows what a window should be. `explain` says so.

