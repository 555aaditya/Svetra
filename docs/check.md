# `svetra check`: find the leaks in an existing app, version 0.1

This document specifies `svetra check`, which reads the access configuration of an existing Supabase (or plain PostgreSQL) database and reports, in plain English, where data is open to people who shouldn't reach it. It needs no `svetra.yaml`: it's for apps that weren't built with Svetra. When the code and this document disagree, that's a bug in one of them.

**Done when:** pointed at a sample vibe-coded app, it finds the planted leak and says how to fix it.

## Command

```sh
svetra check --db URL        [--schema public,...] [--json]
svetra check --local         [--schema public,...] [--json]
svetra check --snapshot FILE [--schema public,...] [--json]
svetra check --print-query   [--schema public,...]
svetra check --migrations [DIR] [--scratch URL] [--strict] [--schema public,...] [--json]
```

- **`--db URL`** reads the catalogs through `psql URL`. Svetra runs one read-only query; it never writes. Use a connection string with read access to the catalogs, such as the one in the Supabase dashboard.
- **`--local`** is `--db postgresql://postgres:postgres@127.0.0.1:54322/postgres`, the database `supabase start` runs.
- **Without `psql`:** `--print-query` prints the query. Run it in the Supabase SQL editor, save the single JSON value it returns to a file, and pass that file with `--snapshot`.
- **`--schema`** lists the schemas the API exposes. Default: `public`. Only objects in these schemas are checked, because only they are reachable through the generated API.
- **`--migrations [DIR]`** checks what your migrations build, without a live database (see below). `DIR` defaults to `supabase/migrations`.
- **Exit codes:** `0` no critical finding, `1` at least one critical finding, `2` a usage error or an unreachable database. With `--migrations`, also `2` when Docker is unavailable (and no `--scratch` is given), when `DIR` is missing or holds no `.sql` file, when none of the migrations applied (nothing was checked), and, only with `--strict`, when any migration failed to apply. A critical finding still exits `1`.

## What it reads

One query returns one JSON document, the **snapshot**:

| Key | For each | Fields |
|---|---|---|
| `tables` | ordinary and partitioned table in a checked schema | schema, name, `rls` (row-level security on), and for `anon` and `authenticated`: `select`, `insert`, `update`, `delete` privileges |
| `policies` | policy on those tables (`pg_policies`) | schema, table, name, permissive, roles, command, `using`, `check` (deparsed expressions) |
| `views` | view and materialized view in a checked schema | schema, name, `security_invoker`, `anon_select`, `authenticated_select`, the definition |
| `functions` | function in a checked schema | schema, name, arguments, `security_definer`, `search_path_set`, `anon_execute`, `authenticated_execute` |

A database without the `anon` or `authenticated` roles (plain PostgreSQL) reports every privilege of theirs as `false`.

## Findings

Each finding has a stable `kind`, a severity, the object, a message in plain English and a fix. Policies granted to `public` apply to `anon` too, and `anon` is reported as "anyone on the internet".

| Kind | Severity | When | Message, for example |
|---|---|---|---|
| `rls_off` | critical | A table's row-level security is off, and `anon` or `authenticated` has a privilege on it | "Anyone on the internet can read every row of public.orders: row-level security is off." |
| `open_write` | critical | A permissive `UPDATE`, `DELETE` or `ALL` policy for `anon` or `public` whose condition is `true` | "Anyone on the internet can change every row of public.orders (policy anyone_edits)." |
| `view_bypass` | critical when the view reads `auth.users`, otherwise warning | A view readable through the API without `security_invoker` | "public.order_summary runs with its owner's rights, so it skips the row-level security of the tables it reads." |
| `open_insert` | warning | A permissive `INSERT` policy for `anon` or `public` whose check is `true`: people can add rows but not change existing ones, which is often a sign-up or contact form | "Anyone on the internet can add rows to public.contact (policy anyone_adds). Fine for a sign-up or contact form; a problem otherwise." |
| `open_read` | warning | A permissive `SELECT` or `ALL` policy for `anon` or `public` whose condition is `true` | "Anyone on the internet can read every row of public.posts (policy public_read). Fine for public data; a leak otherwise." |
| `signed_in_write` | warning | The same `true` `UPDATE`, `DELETE` or `ALL` policy for `authenticated` (an open `INSERT` for signed-in people is ordinary and not reported) | "Every signed-in person can change every row of public.orders." |
| `user_metadata` | warning | A policy's condition reads `user_metadata` or `raw_user_meta_data` | "Policy is_admin trusts user_metadata, which every person can change about themselves." |
| `definer_function` | warning when `anon` can call it, otherwise info | A `SECURITY DEFINER` function the API can call | "Anyone on the internet can call public.transfer, which runs with its owner's rights. Check it validates the caller." |
| `mutable_search_path` | warning | A `SECURITY DEFINER` function without a fixed `search_path` | "public.transfer has no fixed search_path, so a caller can make it run their own code." |
| `no_policy` | info | Row-level security is on, the API has privileges, and there are no policies | "Nobody can reach public.audit through the API: row-level security is on with no policies. Intended?" |

A table, view or function the API roles have no privileges on is never reported: it can't be reached.

### A policy written for the service role

In a hand audit of the policies `check` flagged in 300 public repos, more than half of the open writes were policies named for the backend ("Service role manages sessions", "System can update rate limits", "Service role has full access") with a condition of `true` and **no `to` clause**. A policy with no `to` applies to every role, so it is open to `anon`, not just to the backend. It is also never needed: the service role has `bypassrls`, so it skips row-level security whether or not a policy lets it in.

This adds no new kind and changes no severity. `open_write`, `open_read` and `signed_in_write` fire exactly as before, from the roles and the condition. What changes is the **explanation and the fix**, when the policy's name suggests it was meant for the backend:

- **When:** the finding comes from a permissive policy whose name, lower-cased, contains `service role`, `service_role`, `service`, `system` or `backend`, and whose roles do not include `service_role`.
- **Message:** the usual sentence, then "The policy's name says it is for the service role, but it has no `to service_role`, so it applies to everyone."
- **Fix:** "Delete the policy: the service role skips row-level security and doesn't need one. Or add `to service_role` to it." This replaces "Replace `true` with a condition on who owns the row".
- **Why only the words, not the severity:** the finding is structural and stands without the name. The name only says what the author meant, which lets the fix be the right one. A policy with another name is reported as before, with the generic fix. A name match is never used to suppress a finding.
- **A policy that does list `service_role` among its roles** is not given this wording, and is reported only if it also lists `anon`, `public` or `authenticated`, as before.

Example:

```
critical  public.sessions
          Anyone on the internet can change every row of public.sessions (policy Service role manages
          sessions). The policy's name says it is for the service role, but it has no `to service_role`,
          so it applies to everyone.
          fix: delete the policy: the service role skips row-level security and doesn't need one,
               or add `to service_role` to it.
```

**Tests when this is built:** a policy `service_manages` on `public.comments` for all, no `to`, `using (true)`, in `tests/pg/check/leaky_app.sql` and the snapshot fixture; the golden JSON shows the service-role wording; a unit test that a policy with the same condition and an unrelated name keeps the generic fix, and one that has `to service_role` is not reported at all.

## Output

```
svetra check: 2 critical, 1 warning in public (postgresql://…@db.example.supabase.co/postgres)

  critical  public.orders
            Anyone on the internet can read every row of public.orders: row-level security is off.
            fix: alter table public.orders enable row level security; then add policies,
                 or describe the table in svetra.yaml and let Svetra write and test them.
  ...
```

`--json` prints `{ "findings": [{ "kind", "severity", "object", "message", "fix" }], "counts": {...} }`.

Connection strings are never printed with their password.

## Checking migrations, without a live database

Most Supabase projects keep their schema as SQL files in `supabase/migrations`. `svetra check --migrations` builds that schema in a throwaway database, then runs the same checks on it. It is for a repository, a pull request or CI, where there's no database to point at, and for anyone who wants an answer before deploying.

```sh
svetra check --migrations                      # supabase/migrations, in a throwaway Docker database
svetra check --migrations path/to/migrations
svetra check --migrations --scratch URL        # use a database you provide instead of Docker
```

**How it works**

1. **A throwaway database.** By default Svetra starts one PostgreSQL 16 container with a stand-in for Supabase (below) and removes it afterwards; if the run is interrupted, the container stops itself after 15 minutes and goes with it. The container has no network, no extra privileges, 1 GB of memory and a limit on processes, because migrations are SQL that runs with a superuser's rights. Nothing is ever run against your real database, and nothing is written to your project.
2. **`--scratch URL`** uses a database you provide instead, for machines without Docker (it needs `psql`). Svetra refuses unless it is empty (no tables, views or functions in the checked schemas), prints that it will run your migrations in it, and leaves them applied. It does **not** install the stand-in, so the database should already be Supabase-like: a fresh `supabase start` database works, and then the check runs on the real thing. Point it at a throwaway database, never at one with data.
3. **Applying.** Every `*.sql` file in `DIR` is applied in file-name order (Supabase names them `<timestamp>_<name>.sql`, so that is the order it runs them). Each file runs in one transaction with `ON_ERROR_STOP`, as `supabase db reset` does. **A file that fails is rolled back whole, and the next files still run.** Supabase would stop at the first failure; Svetra keeps going, because a partial answer beats none and the report says plainly what it could not read.
4. **Checking.** The snapshot is read from the throwaway database through the same query as `--db`, and the findings are the ones above, unchanged.

**What it prints.** The usual report, preceded by one line about how much it could read, and followed by the failures:

```
svetra check: 2 critical, 1 warning in public (14 migrations, 12 applied)

  critical  public.orders
            ...

2 migrations could not be applied, so the findings above may be incomplete:
  20260310120000_add_vectors.sql   extension "vector" is not available
  20260312090000_reports.sql       relation "public.legacy_orders" does not exist

This reads your migrations, not your live database. A table fixed in the dashboard, or made
outside the migrations, isn't seen.
```

A failure shows the file name and the first line of the database's error, never the SQL. A file over 2 MB is reported as failed without being run. `--json` adds `"migrations": { "files": 14, "applied": 12, "failed": [{ "file", "error" }] }` and leaves everything else as it is. If nothing applied, the report says so and no finding is reported as clean.

**The Supabase stand-in** is the roles (`anon`, `authenticated`, `service_role`, `authenticator`, `supabase_*`), the `auth`, `storage`, `extensions`, `realtime` and `graphql_public` schemas, `auth.uid()`, `auth.jwt()`, `auth.role()` and `auth.email()`, the `uuid-ossp` and `pgcrypto` extensions, and the default privileges Supabase gives `anon` and `authenticated` in `public`. It lives in two SQL files in the repository (`src/standin/base.sql` and `extra.sql`), compiled into the binary and shared with the access tests' harness and the leak study. It is not Supabase: extensions that aren't in the stock image (`pg_net`, `pg_cron`, `vector`) fail with "not available", and those migrations are reported as failed.

**What it can and can't tell you.** In a study of 263 public Supabase repositories, 189 had at least one migration that did not apply, most often because a table was created outside the migrations (`relation does not exist`). So expect a partial answer on some projects, and read the failures list. Findings describe what the migrations create, which is not necessarily what is deployed. Two hand audits of the study's findings found no false positive in the sample, and on a real Supabase the critical findings were identical to the stand-in's in all 8 repositories tested, each of which applied fully; that sample is small.

**Done when:** a set of migrations made by splitting the leaky app (`tests/pg/check/leaky_app.sql`) into three files gives the same findings as that app's golden file; a set with a failing file reports the others and lists the failure; `--strict` exits 2 on it; and a run on a machine without Docker gives the plain message `Docker isn't running. Start Docker, or use --scratch with a throwaway database.`

## Relation to Supabase's Security Advisor

Supabase's dashboard lints overlap with several findings (row-level security off, security definer views, `auth.users` exposed, `user_metadata` in policies, mutable search paths). `svetra check` adds plain-English output a non-coder can act on, a command that runs anywhere (CI, an agent, offline from a snapshot) with stable kinds and exit codes, and the path to fixing it for good: a design in `svetra.yaml` whose rules are generated and tested.

## Not in version 0.1

- `svetra infer`, which drafts a `svetra.yaml` from the snapshot (M7, next).
- Storage buckets and their policies.
- Seeding (`seed.sql`), the declarative `supabase/schemas` folder, Edge Functions, and a Supabase config's extension list: only the migrations are read.
- Applying migrations to a real Supabase to check them there; the stand-in is the only throwaway database in 0.1 of this mode.
- An MCP tool for `--migrations`.
- Judging conditions other than `true` and `user_metadata`: a policy like `owner = auth.uid()` is trusted as written.
