# `svetra doctor`: the pre-launch checklist, version 0.1

This document specifies `svetra doctor`, one command to run before launching a Supabase app. It reads the project folder, the project's configuration and, when you point it at one, the database. It reports in plain English what would hurt you after launch: data open to the internet, secret keys in code, risky sign-in settings, public file buckets, generated files that no longer match the design, and what to back up. It needs no `svetra.yaml`: it's for apps that weren't built with Svetra too. When the code and this document disagree, that's a bug in one of them.

**Done when:** pointed at a sample app with a planted open table, a planted service key in front-end code and a committed `.env`, it finds all three, never prints the key, and says how to fix each.

## Command

```sh
svetra doctor [DIR] [--db URL | --local | --migrations [MIGRATIONS_DIR]]
              [--scratch URL] [--strict] [--schema public,...] [--json]
```

- **`DIR`** is the project folder. Default: the current folder.
- **The database source** is the same as in [`svetra check`](check.md). At most one may be given:
  - **`--db URL`** reads the catalogs through `psql`.
  - **`--local`** reads the database `supabase start` runs.
  - **`--migrations [MIGRATIONS_DIR]`** builds the migrations in a throwaway database (Docker, or `--scratch URL`) and reads that. `MIGRATIONS_DIR` defaults to `DIR/supabase/migrations`.
  - **No source:** the database is not checked. Doctor says so on its first line and in its last, and never calls the project ready (see Output). It still runs every check that reads the folder.
- **`--scratch`, `--strict`, `--schema`** mean what they mean in `check`. `--scratch` and `--strict` need `--migrations`.
- **`--json`** prints the report as JSON (see Output).
- **No `--snapshot`** in version 0.1: bucket data (below) isn't in `check`'s snapshot.

**Exit codes**, the same as `check`:

| Code | When |
|---|---|
| `0` | No critical finding |
| `1` | At least one critical finding |
| `2` | A usage error; `DIR` isn't a folder; a database that can't be reached. With `--migrations`, also everything `check` exits 2 for: Docker unavailable (and no `--scratch`), a missing or empty migrations folder, no migration applying, and, only with `--strict`, any migration failing. A critical finding still exits `1` |

Warnings, info and reminders never change the exit code.

## How it relates to `check`

**Doctor is `check` plus the project-folder checks.** It runs `check` on the source you give it, with the same query, kinds, severities, messages and fixes ([check.md](check.md)), and shows them under "Data exposure". Doctor does not re-implement or re-word them: a change to `check` is a change to `doctor`. `svetra check` stays the command for a database alone, for CI, and for agents that only want the leaks.

The rest of doctor is new:

| Area | Reads | Needs a database |
|---|---|---|
| Data exposure | `check`'s findings | yes |
| Leaked keys | the project folder and `git ls-files` | no |
| Auth settings | `supabase/config.toml`, `svetra.yaml` | no |
| Backups and recovery | the project folder | no |
| Storage buckets | `storage.buckets`, in the same read as the data checks | yes |
| Generated files | `svetra.yaml` and the files `compile` wrote | no |

## What it never does

- **No network calls.** Doctor talks to nothing but the database you point it at (through `psql`) or the Docker container it starts. It doesn't look up whether a key is live, whether a package is vulnerable or which plan you are on.
- **It never writes.** Not to your project, not to `supabase/config.toml`, not to git. With `--migrations`, the only thing written is the throwaway database, as in `check`.
- **It never prints a secret.** A finding about a key names the file, the line and the kind of key, and nothing of the value: no prefix, no last four characters, no length. The report, `--json` and error messages all follow this. Connection strings are printed without their password, as in `check`.
- **It never runs your code,** hooks or scripts, and never reads outside `DIR` (it doesn't follow a link that leaves it).

## Findings and output

Every finding has a stable `kind`, a severity (`critical`, `warning` or `info`, as in `check`), an `area`, an object (a database object, or `file:line`), a message in plain English and a fix.

| Area | Kinds | Severity |
|---|---|---|
| `data` | every kind of `check` | as in `check` |
| `keys` | `service_key_exposed`, `secret_exposed`, `jwt_secret_exposed`, `db_password_exposed`, `env_committed`, `env_not_ignored`, `config_secret`, `jwt_unrecognized` | critical, warning or info |
| `auth` | `email_unconfirmed`, `weak_password`, `anonymous_sign_ins`, `open_signup`, `admin_no_mfa`, `mfa_off`, `redirect_open` | warning or info |
| `storage` | `public_bucket` | warning |
| `generated` | `stale_generated`, `design_invalid` | warning |
| `backups` | none: reminders, not findings | none |

**Data exposure** is `check`'s output, unchanged, with the area `data`. Nothing else in this document applies to it.

**Reminders** are things doctor can't read and you should do. They are listed at the end, are never counted and never change the exit code.

```
svetra doctor: 2 critical, 3 warnings, 1 note
  Database: 14 migrations, 12 applied (reads your migrations, not your live database)
  Project:  /home/me/shop (git: yes, svetra.yaml: yes)

Data exposure
  critical  public.orders
            Anyone on the internet can read every row of public.orders: row-level security is off.
            fix: ...

Leaked keys
  critical  src/lib/supabase.ts:4
            ...

Auth settings
  warning   supabase/config.toml:48
            ...

Reminders (doctor can't check these)
  - Backups: ...

Not checked
  - Storage buckets: no database given.

Before launch: fix the 2 critical findings first. Not ready.
```

**The last line** is one of three, and only the third calls the project clear:
- "Fix the N critical findings first. Not ready." with at least one critical finding.
- "No critical findings, but N parts were not checked (listed above). Not a clearance." when any part was skipped: no database given, no git, no `svetra.yaml`, or migrations that failed to apply.
- "No critical findings in everything doctor can check. The reminders above are yours." otherwise.

`--json` prints `{ "findings": [{ "kind", "severity", "area", "object", "message", "fix", "file", "line" }], "counts": { "critical", "warning", "info" }, "reminders": [{ "id", "message" }], "not_checked": [{ "area", "why" }], "ready": false }`. `file` and `line` are present only for folder findings, and `migrations` is added as in `check`. `ready` is true only for the third last line. No message, fix or error contains a secret's value.
