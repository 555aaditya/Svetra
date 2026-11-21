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
| `backups` | `schema_not_in_git`, plus reminders, which aren't findings | warning |

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

## Leaked keys in the project folder

### The anon key is not a leak

Supabase gives every project two keys that are easy to confuse:

- **The anon key** (in newer projects, the *publishable* key, `sb_publishable_...`) is **public by design.** It goes into your front-end code and ships to every visitor's browser, so anyone can read it. It only lets a caller act as `anon`, and what `anon` can reach is decided by row-level security. A leaked anon key isn't a leak; an open table is, and `check` reports that.
- **The service role key** (in newer projects, the *secret* key, `sb_secret_...`) **skips row-level security entirely.** Whoever holds it can read, change and delete every row. It belongs on a server, never in code that runs in a browser.

So doctor never flags the anon or publishable key, in any file. Flagging it would teach people to ignore the report. How it tells the two apart:

- **A JWT** is three base64url parts joined by dots, the first starting with `eyJ`. Doctor decodes the middle part locally (no signature check, no network) and reads the `role` claim. `service_role` is a secret. `anon` is public and skipped. A JWT with no `role`, or any other role, gets `jwt_unrecognized` (info), never `service_key_exposed`.
- **A new-style key** is judged by its prefix: `sb_secret_` is a secret, `sb_publishable_` is public. UNVERIFIED: these prefixes are from Supabase's announced key format and must be checked against the Supabase docs before the build.
- **A variable's name is never enough.** `SUPABASE_KEY=eyJ...` is judged by what the value decodes to. A variable named `SUPABASE_SERVICE_ROLE_KEY` holding an anon key is not flagged. A value that decodes to `service_role` is flagged under any name.
- **Placeholders are skipped:** an empty value, `env(NAME)` or `${NAME}` references, `your-...`, `<...>`, `xxx...` and the like. A key that decodes to `service_role` is still flagged in `.env.example`, `.env.sample` and `.env.template`: they are committed on purpose, so a real key there is a leak.

### Where doctor looks

Every text file under `DIR` up to 1 MB, except `.git`, `node_modules`, `target`, `dist`, `build`, `.next`, `vendor`, lockfiles and binary files. If `DIR` is in a git repository, doctor asks git (`git ls-files`, and `git check-ignore` for the rest) which files are **tracked** (in the repository, so in its history and on GitHub once pushed), **ignored**, or neither. Without git it can't tell, says so under "Not checked", and treats every secret as if its file were tracked.

A file is **front-end** when it ships to the browser:
- it's under `public/`, `static/` or `www/`, or it's `.html`;
- it's `.js`, `.jsx`, `.ts`, `.tsx`, `.vue`, `.svelte` or `.astro` and not under a folder named `server`, `api`, `functions`, `scripts` or `supabase/functions`;
- it's an environment variable whose name starts with `NEXT_PUBLIC_`, `VITE_`, `REACT_APP_`, `EXPO_PUBLIC_`, `PUBLIC_` or `NUXT_PUBLIC_`, because those are put into the browser bundle on purpose. UNVERIFIED: the full list of prefixes per framework.

Any other file is **server-side**.

### The checks

| Kind | Severity | When | Message, for example |
|---|---|---|---|
| `service_key_exposed` | critical | A service role or `sb_secret_` key in a front-end file, or in **any tracked file** (any file when git is unavailable) | "src/lib/supabase.ts:4 holds your Supabase service role key. It skips every row-level security rule, and everyone who opens your site can read it." |
| `secret_exposed` | critical, warning for a Stripe test key | A secret from the list below, in a front-end or tracked file | "server/pay.ts:12 holds a Stripe live secret key. Anyone who can read this file can charge and refund your customers." |
| `jwt_secret_exposed` | critical | A value for `SUPABASE_JWT_SECRET`, or `jwt_secret` in `config.toml`, that isn't an `env(...)` reference, in a front-end or tracked file | "supabase/config.toml:21 holds your project's JWT secret. With it anyone can sign in as any user, admins included." |
| `db_password_exposed` | critical | A connection string with a password (`postgres://user:password@host/db`) in a front-end or tracked file. The local database (`127.0.0.1`, `localhost`) and placeholders (`[YOUR-PASSWORD]`, `password`) are skipped | ".env:3 holds your database password. It gives full access to every table." |
| `env_committed` | critical when the file holds a secret named above, otherwise warning | A tracked `.env`, `.env.local`, `.env.production` and so on, but not `.env.example`, `.env.sample` or `.env.template`. The warning is for a file with any value that isn't public by design: a name containing `SECRET`, `PASSWORD`, `TOKEN` or `PRIVATE`, or a `KEY` that isn't the anon or publishable key | "`.env.production` is committed to git. It holds 2 values that aren't public by design. Deleting it now doesn't remove it from history." |
| `env_not_ignored` | warning | A `.env*` file that exists, isn't tracked and isn't ignored: one `git add .` from being committed | "`.env.local` isn't in .gitignore. One careless `git add .` would commit it." |
| `config_secret` | critical when tracked or git is unavailable, warning when ignored | In `supabase/config.toml`, a key named like a secret (`secret`, `pass`, `password`, `api_key`, `auth_token`, `client_secret`) whose value is a literal instead of `env(NAME)` | "supabase/config.toml:58 holds an OAuth secret in plain text. Use `env(SUPABASE_AUTH_EXTERNAL_GOOGLE_SECRET)` and keep the value in a `.env` file that isn't committed." |
| `jwt_unrecognized` | info, at most one per file | A JWT-looking string in a front-end or tracked file that decodes but has no `role`, or a role other than `anon` and `service_role`, outside paths containing `test`, `fixture`, `mock` or `__snapshots__` | "docs/notes.md:9 holds a signed token Svetra doesn't recognize. If it's someone's session, sign them out." |

`env(NAME)` is the Supabase CLI's way of reading a value from the environment. Svetra's compiler already writes OAuth secrets into `config.toml` that way ([compiler.md](compiler.md#sign-in-settings)).

**Other secrets** (`secret_exposed`) are a short list of formats specific enough to give almost no false alarms: Stripe `sk_live_`, `rk_live_` and `whsec_` (and `sk_test_`, the warning); AWS access key ids (`AKIA` and 16 capital letters and digits); GitHub `ghp_` and `github_pat_`; Supabase personal access tokens (`sbp_` and 40 hex characters); and `-----BEGIN ... PRIVATE KEY-----`. UNVERIFIED: the exact lengths and prefixes, to be checked against each vendor's documentation. A format that can't be made specific stays out. Adding a vendor is a table row and a test.

**The fix** is the same for every key found in a tracked file, and doctor says so: **remove the key from the code, rotate it (Supabase dashboard, Settings, API; or the vendor's dashboard), and only then delete it from git.** A key that was ever committed or shipped must be treated as stolen, because deleting the line deletes neither the history nor the copies. For a front-end file the fix adds: "Do the work that needs this key in a server function (a Supabase Edge Function, or your own backend), and have the front end call that with the signed-in person's token. The front end only needs the anon key."

### How doctor avoids false alarms

- The anon and publishable keys are never flagged (above).
- A key is judged by what it decodes to or by its prefix, never by its variable name or by looking random. There is no entropy guessing: a long random string isn't reported for being long.
- A server-side, git-ignored `.env` holding the service key is where that key belongs, and isn't reported.
- Placeholders, `env(...)` references and the local database's defaults are skipped.
- One finding per file, kind and line.
- A suppressions file isn't in version 0.1 (see the end), so each rule has to be right on its own.

**A finding never contains the value.** Example:

```
critical  src/lib/supabase.ts:4
          This line holds your Supabase service role key. It skips every row-level security rule,
          and everyone who opens your site can read it.
          fix: remove it from the code and rotate it now (Supabase dashboard, Settings, API).
               Do the work that needs it in a server function; the front end only needs the anon key.
```

## Auth settings

Doctor reads how people sign in from two places, and nothing else:

- **`supabase/config.toml`**, the Supabase CLI's file. It describes the local project, and `supabase config push` sends it to a hosted one. **Doctor reads the file, not your hosted project.** If you changed a setting in the dashboard and never pushed it, doctor doesn't see it, and says so in each auth finding's fix ("and check the same setting in the dashboard, under Authentication").
- **`svetra.yaml`'s `auth:` section and `roles:`**, where present ([format.md](format.md#authentication)). When `config.toml` exists, its value wins, because it's what runs. When it doesn't exist, the `auth:` values are used, so a design is checked before `compile` has written anything.

A key missing from `config.toml` has Supabase's default, which doctor applies: sign-ups open, passwords of 6 characters, no email confirmation, anonymous sign-ins off. Doctor never needs a `config.toml`: with none and no `auth:` section, auth checks are listed under "Not checked" and doctor says to run `supabase init`.

**Keys.** VERIFIED means the key is one `svetra compile` already writes ([compiler.md](compiler.md#sign-in-settings)). UNVERIFIED means it's from memory of the Supabase CLI's `config.toml` and must be checked against a `config.toml` made by `supabase init` before the build. No Supabase CLI config was available while writing this spec.

| Kind | Severity | Reads | When | Message and fix |
|---|---|---|---|---|
| `email_unconfirmed` | warning | `[auth.email] enable_confirmations` (VERIFIED), or `confirm_email` | Email sign-in is on and confirmations are off (the default) | "People can sign up with an email address they don't own, so anyone can claim anyone's address. fix: set `enable_confirmations = true` (or `confirm_email: true` in svetra.yaml)." |
| `weak_password` | warning | `[auth] minimum_password_length` (VERIFIED), `password_requirements` (VERIFIED) | Length under 8 (Supabase's default is 6) | "Passwords can be as short as N characters. fix: set `minimum_password_length = 10` and `password_requirements = \"lower_upper_letters_digits\"`." A length of 8 or 9 with no requirements is not flagged: it's a choice, not a mistake |
| `anonymous_sign_ins` | warning | `[auth] enable_anonymous_sign_ins` (VERIFIED) | `true`. Supabase gives anonymous people the `authenticated` role, so every "signed-in people" rule applies to them | "Anyone can get a signed-in account without giving an email. Every rule that allows signed-in people now allows them. fix: set it to `false` unless your app needs guests." |
| `open_signup` | info | `[auth] enable_signup` (VERIFIED), or `signup` | `true` (the default) | "Anyone on the internet can create an account. Fine for a public app; for a private one, set `enable_signup = false` and invite people." Always info: it's the default for public apps |
| `admin_no_mfa` | warning | `svetra.yaml` `roles:` | A role named `admin`, `owner`, `superadmin` or `root` without `mfa: true`. Only with a `svetra.yaml` | "The admin role works with a password alone. If a password leaks, so does everything admins can reach. fix: add `mfa: true` to the role and compile." Without `svetra.yaml`, doctor can't know who your admins are: that's a reminder, "Turn on a second factor for admin accounts" |
| `mfa_off` | warning | `[auth.mfa.totp] enroll_enabled`, `verify_enabled` (VERIFIED) | A role has `mfa: true` but either key is `false`: nobody can enroll an authenticator app, so nobody can ever hold the role | "The role admin needs a second factor, but authenticator apps are switched off, so nobody can reach it. fix: run `svetra compile`." |
| `redirect_open` | warning | `[auth] site_url`, `additional_redirect_urls` (UNVERIFIED) | A redirect URL with a wildcard host (`https://*`), `http://` for a non-local host, or `localhost` while `site_url` is a real domain | "Sign-in links may send people to addresses you don't control. fix: list only your own production addresses." UNVERIFIED: the key names and whether wildcard paths such as `https://app.example.com/**` are a safe, normal form (they are expected to be; they're not flagged) |

**Not read, because the exact keys are UNVERIFIED or the setting is hosted:** OTP and email-link expiry, rate limits, CAPTCHA, SMTP settings, and refresh-token rotation. They are listed under "Not in this version".

**Roles and MFA.** `mfa: true` only protects what an `aal2` session is required for. Doctor doesn't re-check that: `svetra compile --check` and the generated access tests do ([compiler.md](compiler.md#second-factors-in-sql)).

## Backups and recovery

**Doctor can't tell whether your data is backed up.** Backups live in your hosted Supabase project, and doctor makes no network calls and reads nothing from the hosted dashboard. It also doesn't guess from your plan: daily backups and point-in-time recovery are hosted-plan settings, and which plans include them changes. UNVERIFIED: Supabase's current plan limits, which is one more reason doctor states no plan facts.

So backups are a **reminder, not a finding**, printed every run when a project looks hosted (there's a `supabase/` folder or a `--db` source), and never counted:

```
Reminders (doctor can't check these)
  - Backups: open Supabase, Project Settings, Database, Backups. Check that backups are on, how many
    days they are kept, and whether point-in-time recovery is enabled. Doctor can't see this.
  - Restore: a backup you have never restored is a hope. Restore one into a new project once.
  - Your own copy: a periodic `pg_dump`, kept somewhere that isn't your Supabase account, covers
    the case where the account itself is lost.
```

What can be read locally, and is a finding:

| Kind | Severity | Reads | When | Message and fix |
|---|---|---|---|---|
| `schema_not_in_git` | warning | `git ls-files`, `DIR/supabase/` | A `supabase/` folder with no tracked `.sql` file under `supabase/migrations/` or `supabase/schemas/`, while the project has tables (any source gave tables) | "Your tables exist only in the live database. If the project is lost, nothing can recreate them. fix: `supabase db pull` writes them as a migration; commit it." |

With no git, or no `supabase/` folder, this check is skipped and listed under "Not checked".

## Storage buckets

Supabase Storage keeps files in buckets, and a **public bucket** serves every file in it to anyone who has the URL, with no sign-in and no policy involved.

**What it reads.** Doctor's database read is `check`'s query plus one more key, `buckets`: for each row of `storage.buckets`, its id, name, `public`, `file_size_limit` and `allowed_mime_types`. `check`'s own query and output are unchanged. It works with `--db`, `--local` and `--migrations` (a migration that inserts into `storage.buckets` is applied to the throwaway database; the stand-in has the table, `src/standin/extra.sql`). A database without a `storage.buckets` table has no buckets, which isn't an error.

| Kind | Severity | When | Message and fix |
|---|---|---|---|
| `public_bucket` | warning | A bucket with `public = true` | "Every file in the bucket avatars can be opened by anyone with its link. Fine for logos and profile pictures; a leak for invoices and uploads. fix: make the bucket private and hand out time-limited signed links, or check that every file in it is meant for everyone." |

Always a warning, never critical: public buckets are normal, and the content decides. A bucket with `file_size_limit` unset or no `allowed_mime_types` is not flagged in this version (see "Not in this version").

Without a database source, this is listed under "Not checked". Doctor does not read `[storage.buckets.*]` tables in `config.toml`: UNVERIFIED that the CLI has such tables, and they describe local buckets only.

## Generated files

If `DIR/svetra.yaml` exists, doctor runs what `svetra compile --check` runs, and reports what it finds. Without a `svetra.yaml` this check is skipped without comment: most projects don't have one.

| Kind | Severity | When | Message and fix |
|---|---|---|---|
| `stale_generated` | warning | `compile --check` finds a generated file (`supabase/schemas/svetra.sql`, `src/svetra.gen.ts`, `supabase/tests/svetra.test.sql`) out of date, hand-edited or missing, or an owned key of `config.toml` that differs | "supabase/schemas/svetra.sql no longer matches svetra.yaml (it was hand-edited, or the design changed). The rules in your database may not be the rules you designed. fix: run `svetra compile`." One finding per file, with the message `compile --check` gives |
| `design_invalid` | warning | `svetra.yaml` has validation errors, so `compile --check` can't run | "svetra.yaml has 3 errors, so doctor couldn't check that your generated files are current. fix: run `svetra validate` and fix them." |

Doctor doesn't validate the design: `svetra validate` does, and doctor only says it couldn't check. Both are warnings, not critical, because they say the design and the files disagree, not that data is open. The data itself is what `check` reads, so the generated access tests and a stale policy file never hide an open table from doctor.

## The first build PR

Doctor ships in three pull requests, each one topic. The spec is this document.

1. **The command and the folder checks.** `src/doctor/` (after `verify` in the dependency order of AGENTS.md; it calls `inspect`, `migrations` and `project::compile`) and the `doctor` arm in `src/main.rs`, with help text. It contains:
   - argument parsing and the exit codes, the report, and `--json`;
   - the data exposure part, by calling `check`'s code, not copying it;
   - the project walk and `git ls-files` and `git check-ignore` through `std::process::Command`, and the leaked-key checks, with a base64url decoder and `serde_json` for the JWT payload (both already in the dependencies, so no new crate);
   - the backups reminders and `schema_not_in_git`, and the generated-files checks;
   - `docs/doctor.md` updated where the build differs, the `--help` text, a README roadmap row marked Done for these checks, and a `CHANGELOG.md` entry.
2. **Auth settings and storage buckets.** A small `config.toml` reader (the compiler's `config.rs` already finds tables and keys line by line; share it rather than add a TOML crate), the auth checks, and the `buckets` key in doctor's query. Before it is written, the UNVERIFIED keys in this document are checked against a `config.toml` made by `supabase init`, and this document is corrected.
3. **Agents.** A `doctor` tool in `svetra mcp` ([agents.md](agents.md)) with the same inputs and the same JSON plus a `summary`, and a line in the Svetra skill: run `doctor` before telling anyone an app is ready.

**Tests in the first PR** (AGENTS.md: one test per finding kind, CLI tests run the real binary):
- A fixture project built at test time, in a temporary folder with a real `git init`, so no secret-shaped string is committed to this repository. Tokens are assembled at run time from pieces (a JWT is built from `base64(header)`, `base64(payload)`, and a dummy signature), because a committed key-shaped string trips GitHub's push protection and every scanner.
- One test per kind of `keys`, `backups` and `generated`: the planted service key in a `.tsx` file; the same key in an ignored `.env` (not reported); a tracked `.env` with and without a secret; a `config.toml` with a literal and with `env(...)`; a placeholder and an example file.
- **The anon key is never flagged:** a test puts an anon JWT and an `sb_publishable_` key in a front-end file, a tracked `.env` and `config.toml`, and asserts no finding.
- **No secret in any output:** a test runs every planted-secret case with `--json` and plain output, and asserts that neither contains the secret, any 8-character slice of it, or its length.
- The data part: the leaky app's golden file (`tests/pg/check/leaky_app.sql`) through `doctor --migrations` gives the same findings as `check`'s, under `data`.
- Exit codes: `0`, `1` (critical), `2` (not a folder, unreachable database, no Docker).
- The last line: no source gives "Not a clearance", and only a full, clean run gives `ready: true`.

## Not in version 0.1

- **Anything on the network:** whether a found key is still live, a leaked-password check, DNS, HTTPS or CORS settings of the deployed site, and your hosted project's actual settings, backups and plan. Doctor reads files and one database.
- **Hosted-project auth settings** that are not in `config.toml`: OTP expiry, rate limits, CAPTCHA, SMTP, refresh-token rotation, JWT expiry.
- **Storage:** the policies on `storage.objects`, file-size and file-type limits of a bucket, and `[storage.buckets.*]` in `config.toml`.
- **`--snapshot`:** a saved snapshot doesn't carry buckets. A `doctor --print-query` is a possible follow-up.
- **Edge Functions' code** beyond the key checks, which already cover `supabase/functions` as server-side files; their `verify_jwt` settings; dependency vulnerabilities; and security-header checks.
- **Git history:** doctor reads tracked files as they are now. A key that was committed and then deleted is invisible to it, which is why its fix always says to rotate.
- **A suppressions file** to silence a finding on purpose. It waits for real false alarms to show what it needs; until then a false alarm is a bug in this document.
- **Fixing anything.** Doctor reports. `svetra compile` writes, and the fixes it prints are for you or your agent.
