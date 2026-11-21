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

<!-- next -->
