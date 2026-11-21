# Automations, proposal for version 0.2

This document specifies three new kinds of hole: **scheduled**, **triggered** and **webhook**. It is roadmap milestone M4b. It is a proposal: nothing here is built yet, and once it is, [`format.md`](format.md) and [`compiler.md`](compiler.md) become the authority and this file is folded into them.

Real apps soon need code that runs without a person clicking: clean up every night, send a receipt when an order is paid, accept a payment provider's call. Today a hole only runs when someone calls it. These three kinds run on a clock, on a data change, or on an outside call. They keep the rules holes already have: declared access, a role of their own, and generated tests that prove it.

**Done when:** see [Done when](#done-when) at the end.

## What stays the same

All three kinds are holes, so everything in [format.md](format.md#holes) still applies:

- The body is hand-written SQL statements in `supabase/holes/<name>.sql`, statements only (`SVT032`), created as a stub and never overwritten.
- `reads`, `writes` and `grants` work as they do now. The body runs as the role `svetra_hole_<name>`, which holds exactly those privileges.
- The body function lives in `svetra_holes`, is `security definer`, is owned by the hole's role and has `set search_path = ''`.
- Names are snake_case and unique across the file (`SVT021`, `SVT024`).

What changes is **who or what starts the hole**. A `function` hole has a caller. These three don't, so `as` means nothing for them, and the body's `p_caller` is `null` unless this document says otherwise.

| Kind | Started by | Reachable from the API |
|---|---|---|
| `function` (today) | `supabase.rpc('<name>')`, by a person or visitor | Yes, as `as` says |
| `schedule` | The database's own clock | **No** |
| `trigger` | A row being inserted, updated or deleted | **No**, except by making the change |
| `webhook` | An HTTPS call from outside | **Only through its generated Edge Function**, which checks a secret first |

## What Supabase supports

Anything not confirmed from the docs or the pinned CLI in this environment is marked **UNVERIFIED**. The first build PR must verify these rows on a real local Supabase (`scripts/supabase-check.sh`) and edit this table.

| Fact | Status |
|---|---|
| `pg_cron` is a Postgres extension offered on hosted Supabase, switched on per project (Dashboard, Database, Extensions, or `create extension`) | Believed true, **UNVERIFIED** |
| `pg_cron` is present in the local Supabase CLI stack | **UNVERIFIED** |
| `cron.schedule(job_name, schedule, command)` creates a job, or replaces the one with the same name; `cron.unschedule(job_name)` removes it | Believed true (pg_cron's own API), **UNVERIFIED** on Supabase |
| A job runs inside the database, as the role that scheduled it | Believed true, **UNVERIFIED** on Supabase |
| Schedules are five-field cron, in UTC. Whether the time zone can be changed on Supabase is unknown, so 0.2 supports UTC only | **UNVERIFIED** |
| pg_cron also takes `N seconds` schedules. Not used in 0.2 | **UNVERIFIED** |
| Many jobs at the same moment is discouraged (Supabase suggests a small number, about 8) | **UNVERIFIED**; Svetra doesn't enforce it |
| Edge Functions are Deno files in `supabase/functions/<name>/index.ts`, deployed with `supabase functions deploy`, with secrets from `supabase secrets set` read through `Deno.env` | Believed true, **UNVERIFIED** here |
| An Edge Function gets `SUPABASE_URL` and `SUPABASE_SERVICE_ROLE_KEY` in its environment | Believed true, **UNVERIFIED** |
| `[functions.<name>] verify_jwt = false` in `supabase/config.toml` lets a caller with no Supabase token reach the function | Believed true, **UNVERIFIED** |
| Postgres triggers work as in plain PostgreSQL | True; CI already runs PostgreSQL 16 |

`pg_cron` and Edge Functions can't run on the plain-PostgreSQL stand-in of `scripts/pg-check.sh`. So tests come in two layers: the SQL shape on the stand-in (with a stub `cron` schema, like the existing Supabase stand-in), and real behaviour only in `scripts/supabase-check.sh`.

## Common rules

### Who the code runs as

| Kind | Started by | Body runs as |
|---|---|---|
| `function` | `authenticated`, `anon` or both | `svetra_hole_<name>` |
| `schedule` | the migration's role, through pg_cron | `svetra_hole_<name>` |
| `trigger` | the role that made the change | `svetra_hole_<name>` |
| `webhook` | `service_role`, from the Edge Function | `svetra_hole_<name>` |

The last column is the same on purpose: whatever starts it, the code gets the same fence.

### Secrets

- **Never in `svetra.yaml`,** in a hole body or in a generated file. The design holds only the **name** of a secret.
- The Edge Function reads the value from its environment at run time. `svetra compile` never reads a secret.
- `explain` lists the secrets to set before going live, by name.
- Scheduled and triggered holes need no secret: nothing outside the database starts them.

### Roles that automations can give

A hole with `grants` can give and take away roles. With no person behind a schedule or a trigger, that means a role handed out by a clock or a row. It is allowed ("make whoever accepted the invite a member" is a real trigger), but it gets a warning (`SVT040`) and its own line in `explain`.

## Scheduled holes

A scheduled hole runs on a clock.

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
| `schedule` | yes | Five-field cron: minute, hour, day of month, month, day of week. UTC. Lists, ranges and steps are allowed (`*/15`, `1-5`, `1,15`). Names such as `mon` are not, so a field means one thing |
| `reads`, `writes`, `grants` | no | As for `function` |

Not allowed, because nobody passes or receives them: `input`, `output`, `as`, `when`. Using one is `SVT020`, and the hint says why. A guard on a scheduled hole is written in the body (`if not exists (...) then return; end if;`), where its author can see it.

### What Svetra generates

All of it goes in the `-- Holes` section of the SQL schema.

1. **The role** `svetra_hole_<name>` and its grants, exactly as for a `function` hole.
2. **The body function** `svetra_holes.<name>(p_caller uuid)`, with the usual attributes and no guard. `p_caller` is always `null`; it stays in the signature so a body can move between kinds.
3. **A job**, scheduled in the same migration:
   ```sql
   create extension if not exists pg_cron;   -- UNVERIFIED: schema and spelling on Supabase
   select cron.schedule('svetra_expire_invites', '0 3 * * *',
     $job$select svetra_holes.expire_invites(null)$job$);
   ```
   The job runs as the migration's role (`postgres` on Supabase), the only role allowed to execute the body function. The body still runs as `svetra_hole_<name>`, so the job gives it no extra access.
4. **A single-run guard.** The job takes `pg_try_advisory_xact_lock` on the hole's name and returns at once if the last run hasn't finished. Two overlapping runs of a cleanup are a usual way a nightly job corrupts data.
5. **Cleanup of removed jobs.** Every job Svetra creates is named `svetra_<name>`. The schema ends with a statement that unschedules each `svetra_%` job that the design doesn't declare. Jobs you made by hand under other names are never touched.

**There is no function in `public`,** so nothing in the API can call a scheduled hole, and `src/svetra.gen.ts` gets nothing for it.

**The stub body** raises `expire_invites is not written yet: edit supabase/holes/expire_invites.sql`. Until it is written the job fails each run and says why in pg_cron's run log. It never changes data.

### Failure and time

- A failing body rolls back its own run. Svetra adds no retry: the next scheduled run is the retry.
- Svetra can't know about a run missed while the database was down. A body should be safe to run twice and to run late. [`agents.md`](agents.md) says so.

## Triggered holes

A triggered hole runs after a row of one entity is inserted, updated or deleted.

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
| `events` | yes | A non-empty list of `insert`, `update`, `delete` (`SVT029` otherwise) |
| `when` | no | A condition on the row that decides whether the body runs. Default: always |
| `reads`, `writes`, `grants` | no | As for `function` |

Not allowed: `input`, `output`, `as`. The row is the input.

Timing is always **after** the row is written. A `before` trigger can change or cancel the write, which is a rule about data, so it belongs in a constraint or a policy, not a hole.

### The row in the body

| Variable | Insert | Update | Delete |
|---|---|---|---|
| `p_new` | the new row | the row after | `null` |
| `p_old` | `null` | the row before | the deleted row |
| `p_caller` | the person whose change fired it, from the session's `auth.uid()`; `null` for the service role, a job or a migration | same | same |

Both rows are typed as the entity's row type. `p_caller` tells the body who started the change. It is information, never a permission check.

### `when`

`when` becomes the trigger's own `WHEN (...)` clause, so Postgres decides whether to fire before any function runs. That makes it checkable from the catalog (see [tests](#what-the-access-tests-prove)).

- Fields are written bare, as in a policy, and mean the row after the change (the deleted row for `delete`).
- `old.<field>` means the row before. It is allowed only when `events` is exactly `[update]`, because Postgres refuses `old` in an insert trigger's `WHEN` and `new` in a delete trigger's.
- No other entity, no `auth.user`, no `has_role`: a `WHEN` clause can't run a subquery, and there may be no signed-in person. These are `SVT003`, as is a non-boolean result.
- Svetra creates **one Postgres trigger per event**, so each can use the right row. `[insert, update]` is two triggers, `svetra_<name>_insert` and `svetra_<name>_update`.

The expression parser knows the row after a change (`check`). `old.` is the one new form, so the build needs a change in `src/expr.rs` and `src/typeck.rs`.

### What Svetra generates

1. **The role** `svetra_hole_<name>` and its grants.
2. **The body function** `svetra_holes.<name>(p_caller uuid, p_old "public"."order", p_new "public"."order")`, with the usual attributes and no guard: the `WHEN` clause did that.
3. **A trigger function** `svetra_holes.<name>__fire()`, owned by `postgres` and `security definer`. It refuses when `pg_trigger_depth() > 3` (see [loops](#loops)), passes `auth.uid()` as `p_caller`, and calls the body function.
4. **The triggers**, one per event:
   ```sql
   create trigger svetra_notify_paid_update
     after update on "public"."order"
     for each row
     when (new."status" = 'paid' and old."status" is distinct from 'paid')
     execute function svetra_holes.notify_paid__fire();
   ```
   Always `for each row`: a statement trigger has no single row to give the body.
5. **Privileges.** Execute on both functions is revoked from `public`, `anon`, `authenticated` and `service_role` by name and granted to nobody who isn't the owner. In Postgres, the role that fires a trigger doesn't need execute on its function. **UNVERIFIED on Supabase:** the access tests must confirm it there.

No function in `public`, nothing in `src/svetra.gen.ts`.

### Who the code runs as

The hole's own role, **not the person who made the change** and not `postgres`. It touches only what `reads`, `writes` and `grants` declare. That is deliberate: a trigger on orders often writes a notification that the person who placed the order may never see. `explain` says it in words: changing an order can make custom code change notifications, whoever made the change.

### Loops

A trigger hole that writes the entity it fires on, or an entity whose own trigger hole writes back, loops. Svetra stops it twice.

1. **At validate time, `SVT038`.** Draw an edge from each trigger hole to every trigger hole whose `on` entity is in the first one's `writes` and whose `events` Svetra can't rule out. It can't read the body, so writing an entity counts as insert, update and delete. Any cycle, including a hole that writes its own `on` entity, is an error. The message names the cycle. The hint says to write to another entity, merge the two holes, or narrow `events`.
2. **At run time, a depth limit.** The trigger function raises SQLSTATE `54001` when `pg_trigger_depth() > 3`, so a loop through a hand-made trigger, which Svetra can't see, ends in an error and not a hung database. It is a backstop. A design that depends on it is wrong.

Holes of other kinds that write an entity do fire its trigger holes. That is an edge and not a cycle, because nothing fires them back, so only trigger-to-trigger edges are checked.

### Failure and cost

- The trigger runs **in the same transaction** as the change. If the body raises, the person's insert, update or delete is rolled back. That is right for "keep two things consistent" and wrong for "send an email". 0.2 has only this mode.
- A slow body makes every write on the entity slow. Keep it short. Slow or external work is [not in this version](#not-in-this-version).

## Webhook holes

A webhook hole is an incoming HTTPS call from another service: a payment provider reporting a charge, a form tool sending a submission.

### Decision: a generated Edge Function in front of a locked function

There are two ways to receive an outside call on Supabase.

| | PostgREST RPC with a secret in the request | Edge Function that checks, then calls the database |
|---|---|---|
| Where the check runs | In SQL, inside the function | In the Edge Function, before the database is touched |
| Signature over the raw body (HMAC), as most providers send | Hard. PostgREST gives SQL parsed JSON, not the bytes the sender signed. Taking raw bytes depends on PostgREST behaviour that is **UNVERIFIED** here | Natural: read the raw body, compute the HMAC, compare in constant time |
| Where the secret lives | Readable by SQL, so in the database (the Vault, **UNVERIFIED**) or a setting | In `supabase secrets`, never in the database |
| Front door | Anyone with the project's public key, straight into a database function | Anyone on the internet, but a bad call stops before the database |
| Extra moving part | None | One generated file to deploy |

**Decision: the Edge Function.** Providers sign the raw body, and only code that sees the raw body can check it. A check in SQL would have to skip signatures or trust re-serialised JSON, and would put the secret in the database. The cost is one deploy step, which `explain` lists under "Before going live", as it does for sign-in settings.

The Edge Function is **generated in full**, so no hand-written code sits in front of the data. The only hand-written logic is still the SQL body. After the check passes, the function can do one thing: call the hole's locked database function.

```yaml
holes:
  payment_received:
    kind: webhook
    secret: PAYMENTS_WEBHOOK_SECRET   # the NAME of a secret, never its value
    signature:
      scheme: hmac_sha256
      header: x-signature
    input:
      invoice: Invoice.id
      paid: Invoice.amount
      event_id: text
    dedupe: event_id
    reads: [Invoice]
    writes: [Invoice, Payment]
```

| Key | Required | Meaning |
|---|---|---|
| `kind` | yes | `webhook` |
| `secret` | yes | The **name** of the secret that holds the key: `^[A-Z][A-Z0-9_]*$`. Set with `supabase secrets set NAME=...`. Anything else is `SVT039`, so a pasted key is caught |
| `signature` | no | `{ scheme, header }`. `scheme` is `hmac_sha256` (the header is the hex HMAC-SHA256 of the raw body) or `shared_secret` (the header must equal the secret). Default: `hmac_sha256` in `x-signature`. Provider-specific formats are not in this version |
| `input` | yes | Map of name to type, as for `function`. Each is read from the JSON body's top-level key of the same name |
| `dedupe` | no | One `input` name that identifies the event. A second call with the same value is answered `200` and ignored |
| `reads`, `writes`, `grants` | no | As for `function` |
| `when` | no | A condition on the inputs, as for `function`, without `auth.user` or `has_role` (`SVT003`): nobody is signed in |

Not allowed: `as` (always the Edge Function) and `output` (a webhook answers `200` with no body).

### What Svetra generates

1. **The role** `svetra_hole_<name>` and its grants.
2. **The body function** `svetra_holes.<name>(p_caller uuid, p_<input>, …)`: the `when` guard, then the body. `p_caller` is `null`.
3. **The callable function** `public.<name>(…)`, generated as for a `function` hole, but with execute granted **only to `service_role`** and revoked from `public`, `anon` and `authenticated` by name. PostgREST can see it, and only the service key can run it. It stays `security definer`, so what runs the body is the hole's role, not the service role's wide access.
4. **The Edge Function** `supabase/functions/<name>/index.ts`, generated and overwritten each compile, with the "do not edit" header of the other generated files. It:
   - reads the raw body **before** parsing it;
   - checks the signature in constant time against `Deno.env.get('<secret>')`. A missing secret, a missing header or a mismatch is `401` with no detail;
   - parses the JSON and validates it with the same Zod schema as `src/svetra.gen.ts` (`400` on failure);
   - calls `rpc('<name>', …)` with the service key. Success is `200`, a refused guard (`42501`) is `403`, and any other failure is `500` with no detail.
5. **`supabase/config.toml`:** `[functions.<name>]` with `verify_jwt = false`, written in place like the sign-in keys, because the provider has no Supabase token. The signature check takes its place.
6. **Deduplication,** only when some webhook uses `dedupe`: a table `svetra.webhook_events (hook text, event_id text, primary key (hook, event_id))`, owned by `postgres`, with no grants to API roles. The generated `public.<name>` inserts into it with `on conflict do nothing` in the same transaction as the body and returns early when the pair was already there. A failed body rolls the insert back, so the provider's retry runs again.

Deduplication stops duplicate deliveries. It doesn't stop an old, correctly signed call being sent again. Timestamp checks are [not in this version](#not-in-this-version), and `explain` says so.

## Diagnostics

The highest code in use is `SVT036`. `SVT011` to `SVT018` have never been used; they stay free, and new codes continue after `SVT036`. Existing codes are reused wherever they already mean the thing.

| Code | Severity | Rule | Remediation action |
|---|---|---|---|
| `SVT037` | error | A `schedule` isn't a valid five-field cron expression: wrong number of fields, a value out of range, or a name where a number is needed. The message says which field | `fix_schedule` |
| `SVT038` | error | Trigger holes form a loop: one writes an entity whose trigger hole (or its own) fires on it. The message names the cycle | `break_loop` |
| `SVT039` | error | A webhook's `secret` isn't the name of a secret (`^[A-Z][A-Z0-9_]*$`), which usually means its value was pasted into the file. The message never prints the value | `use_secret_name` |
| `SVT040` | warning | A `schedule` or `trigger` hole has `grants`: a role is given or taken away with no person calling it | `confirm_grants` |

Existing codes the new kinds use:

| Code | Used for |
|---|---|
| `SVT020` | A key not allowed for the hole's kind, such as `input` on a `schedule` or `as` on a `webhook`. The hint lists the keys the kind takes |
| `SVT028` | A key the kind requires is missing: `schedule`, `on`, `events`, `secret`, `input` |
| `SVT029` | `events` isn't a list of `insert`, `update`, `delete`, or a `scheme` isn't one of the two |
| `SVT002` | `on` names an entity that doesn't exist |
| `SVT003` | `when` on a trigger or webhook uses what it can't: another entity, `auth.user`, `has_role`, `old.` outside `[update]` |
| `SVT004` | Already counts a hole's `writes`; it now counts all kinds |
| `SVT032` | The body rule, unchanged, for all kinds |

Per [`AGENTS.md`](../AGENTS.md), each new code needs a row in `src/diag.rs`, a row in `format.md`'s Diagnostics table, a test that triggers it, and a message that says what is wrong and how to fix it. The loader's allowed-key constants and `schema/svetra.schema.json` change together (`tests/schema.rs`).

## What the access tests prove

The generated pgTAP tests already prove, for a `function` hole, its table privileges, its guard for every person, and who can run it. Each new kind gets the same style of proof, with test names that read as sentences.

| Kind | Check | Proves |
|---|---|---|
| all | The hole's role can do exactly what `reads`, `writes` and `grants` declare on every entity, and nothing on the others | The fence is the declared one (the existing check, run for every kind) |
| `schedule` | `anon`, `authenticated` and `service_role` can't execute `svetra_holes.<name>` | No API role can run a scheduled hole, or forge its caller |
| `schedule` | No function named `<name>` exists in `public` | There is no `rpc` entry |
| `schedule` | `cron.job` has exactly one job named `svetra_<name>`, with the declared schedule, calling `svetra_holes.<name>` | The clock runs the declared hole on the declared schedule |
| `schedule` | No `svetra_%` job exists that the design doesn't declare | A removed hole's job is gone |
| `trigger` | `pg_trigger` has one trigger per declared event on the declared table: `after`, `for each row`, enabled | It fires when, and only when, declared |
| `trigger` | `pg_get_triggerdef` contains the compiled `when` | The condition is the one in the design |
| `trigger` | The body function is `security definer`, owned by `svetra_hole_<name>`, with an empty `search_path` | It runs as the declared role |
| `trigger` | `anon`, `authenticated` and `service_role` can't execute the body or the trigger function | It can't be started by calling it |
| `trigger` | As its own role, an insert into an entity not in `writes` is denied | A triggered hole can't write beyond its fence |
| `webhook` | `anon` and `authenticated` can't execute `public.<name>`; `service_role` can | Only the Edge Function's key reaches it |
| `webhook` | `anon` and `authenticated` can't execute `svetra_holes.<name>` | As for every hole |
| `webhook` | The guard: inputs that point at the `a` rows and at the `b` rows are refused (`42501`) or let through as `when` says | As for `function` holes |

**Not covered, and said so in `explain` and in the test output:**
- What a body does when it runs. Bodies are hand-written, as for `function` holes.
- The Edge Function's signature check. A **generated Deno test** that signs a sample body and checks accept, reject and wrong-header cases is a follow-up PR, because it needs a Deno runner in CI. Until then the build PR checks it by hand on a real local Supabase and says so.
- That pg_cron fires at the right minute. That is pg_cron's job. The test proves the job is registered correctly.

Each kind also gets deliberate breakages in `tests/pg/breakages/`, like the existing ones, each failing exactly the expected checks: execute on a body function granted to `authenticated`, a job's schedule edited, a trigger's `when` dropped, a hole's role given a privilege it didn't declare.

## How `explain` and `view` describe them

Words follow [`understand.md`](understand.md): no "hole", "trigger" or "cron". They go in the existing **Custom code** section, one entry per hole, each saying what starts it and what it can read and change.

| Kind | `explain` |
|---|---|
| `schedule` | `expire_invites: runs by itself every day at 03:00 (UTC). Nobody can run it from the app. It can read invites and change invites.` |
| `trigger` | `notify_paid: runs by itself after an order is changed, only when its status becomes paid. Nobody can run it directly. It can read orders and customers and change notifications, whoever made the change.` |
| `webhook` | `payment_received: runs when another service calls your app, and only if the call carries the right signature. Nobody can run it from the app. It can read invoices and change invoices and payments.` |

- **Cron in words.** Common forms are spelled out: "every day at 03:00", "every 15 minutes", "every Monday at 09:00", "on the 1st of each month at 00:00". Any other valid expression is shown as written, in quotes, never guessed at.
- **Side effects where readers look.** For a trigger, the entity it fires on gets an extra line under "Who can see and change what": "Changing an order also runs `notify_paid`, which changes notifications." The "Only custom code can change them" line names these holes too.
- **Grants.** "It can give people the member role" gets a second sentence for `schedule` and `trigger`: "Nobody is asked first: it happens by itself." This is the `SVT040` warning in words under "Worth knowing".
- **Before going live** gets, per webhook, the secret's name with its `supabase secrets set` command and the deploy step; and, once for any schedule, "turn on the pg_cron extension in the Supabase dashboard". A `shared_secret` webhook is described as weaker than a signature, and every webhook says old calls can be replayed.
- **`context`** gets one line each, as today: `hole expire_invites: schedule "0 3 * * *"; reads Invite; writes Invite`, `hole notify_paid: trigger Order update when status == 'paid' and old.status != 'paid'; reads Order, Customer; writes Notification`, `hole payment_received: webhook secret PAYMENTS_WEBHOOK_SECRET (invoice Invoice.id, paid Invoice.amount, event_id text); reads Invoice; writes Invoice, Payment`.
- **`graph`** draws them with the same rounded node, and a dotted line from the entity a trigger fires on into the hole, besides the usual `reads` and `changes` edges.
- **`view`:** each Custom code card gets a first row, **Starts**, with the sentence above. The All, Some, No table of who can call it is replaced for these kinds by "Nobody can run this from the app", because there is no person to evaluate. The `--json` model gets `"kind"` and `"starts"` per hole, and `"secret"` (the name only) for a webhook. The page still makes no network requests.
- **`svetra diff` and `plan`** ([change.md](change.md)): adding an automation is **safe**. Changing `schedule`, `on`, `events`, `when` or `secret` is **breaking**, because behaviour changes without a data change. Widening `reads`, `writes` or `grants` is access widening, as for `function` holes.

