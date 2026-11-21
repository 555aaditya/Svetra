# `svetra infer` on the Supabase Slack clone

`study/p7/outside/` ran the mutation study on Supabase's own Slack-clone example against a spec written by hand. This folder asks the other question: what does `svetra infer` (milestones 1 and 2) draft from the same app's database, and how does the draft compare with that hand-written spec and with the app's actual policies?

The app, commit and licence are the ones in [`../README.md`](../README.md) (`supabase/supabase`, `examples/slack-clone/nextjs-slack-clone`, commit `9964968b53f078bf4e246fc25443e3220a8a143b`, Apache-2.0). Only `supabase/migrations/` was read, with `curl` from `raw.githubusercontent.com` at that commit. No live deployment was touched and nobody was contacted.

## Result

| | |
|---|---|
| Tables in `public` | 5 (`users`, `channels`, `messages`, `user_roles`, `role_permissions`) |
| Entities drafted | 5 of 5 |
| Fields drafted | 18 of 18 columns |
| Things left as `UNTRANSLATED` | 22: 14 policies, 4 identity keys, 2 defaults, 2 foreign keys |
| Policies translated | **0 of 14** |
| Entities `private: true` | 5 of 5 (no policy translated anywhere) |
| Wider than the database | **none**. The draft exposes nothing; every entity is private, which is narrower than the database |
| `svetra validate` on the draft | valid: 0 errors, 5 warnings (`SVT004`, one per entity), 1 info `SVT009`, 2 info `SVT010` |
| `svetra compile` on the draft | succeeds with the same diagnostics; all five tables get row-level security on and every API privilege revoked |
| Against the hand-written spec | its 3 entities all have a counterpart; of its 13 fields, 5 identical, 2 same type but a modifier differs, 5 other type, 1 missing from the draft |

Svetra 0.8.2 on `develop` at `ac31de5` (includes `on_delete` on refs, #111). The sibling change for user-typed primary keys is not on `develop`, so it is not in these numbers. Both migrations apply in order to PostgreSQL 16 with the stand-in (`svetra check --migrations` reports 2 applied).

## What each table came out as

| Table | Entity and fields | Policies translated | Why not |
|---|---|---|---|
| `users` | `Users`: `id` uuid primary, `username` text, `status` enum (3 of 3; the foreign key `id` to `auth.users` is a comment) | 0 of 3 | read is `auth.role() = 'authenticated'`; insert and update are `auth.uid() = id`, and `id` is a plain `uuid` in the draft, not a `user` field |
| `channels` | `Channels`: `id` int primary, `inserted_at` timestamp, `slug` text unique, `created_by` ref to `Users` (4 of 4) | 0 of 4 | read is `auth.role()`; insert and delete own are `auth.uid() = created_by`, a `ref` to `Users`, not a `user` field; delete by permission is `authorize('channels.delete')` |
| `messages` | `Messages`: `id` int primary, `inserted_at`, `message` text, `user_id` ref, `channel_id` int (5 of 5; the foreign key with `on delete cascade` is a comment) | 0 of 5 | read is `auth.role()`; insert, update and delete own are `auth.uid() = user_id` on a `ref`; delete by permission is `authorize('messages.delete')` |
| `user_roles` | `UserRoles`: `id` int primary, `user_id` ref, `role` enum, unique `[user_id, role]` (3 of 3) | 0 of 2 | one policy is `to supabase_auth_admin`; the other is `auth.uid() = user_id` on a `ref` |
| `role_permissions` | `RolePermissions`: `id`, `role` enum, `permission` enum, unique `[role, permission]` (3 of 3) | 0 of 0 | row-level security on with no policy: `private: true`, which says the same |

## Entities and fields against the hand-written spec

The hand-written spec changed the app on purpose (listed in `../README.md`) so that the generated tests could run. The draft keeps the app as it is, so each difference below is the draft being truer to the database than the spec is.

| Table | Draft against the spec | Fields |
|---|---|---|
| `users` / `Profile` | `Profile` has its own uuid `id` and a `user_id` of type `user`; the draft has `id` uuid as the primary key and no `user_id` | `id`, `username`, `status` identical; `user_id` only in the spec |
| `channels` / `Channel` | the spec's keys are uuid, the draft's are `int` (the table's bigint identity); `created_by` is `user` in the spec and `ref` to `Users` in the draft (it references `public.users`) | `slug` identical; `inserted_at` same type, but the spec has `auto: now` and the draft leaves `timezone('utc'::text, now())` as a comment; `id`, `created_by` other type |
| `messages` / `Message` | same two differences, and `channel_id` is `int` in the draft, uuid in the spec, with the `on delete cascade` foreign key left as a comment | `message` identical; `inserted_at` as above; `id`, `user_id`, `channel_id` other type |
| `user_roles`, `role_permissions` | not entities in the spec (its roles are Svetra's own) | both drafted as private entities |

A person adopting the draft would still have to decide, by hand:

- **Keys.** Svetra has no auto-increment and a `ref` needs a uuid key, so the four bigint identity keys and `messages.channel_id` stay `int` (the draft says so in comments) and the `messages` to `channels` foreign key is a comment. They would change them to uuid, as the hand spec does.
- **Who owns a row.** The owner columns reference `public.users`, not `auth.users`, so the draft makes them `ref` and not `user`. The owner check `auth.uid() = created_by` only translates on a `user` field, so the person makes that call for `channels.created_by`, `messages.user_id`, `user_roles.user_id` and `users.id`.
- **The read rule** `auth.role() = 'authenticated'` (three policies): `infer` leaves `auth.role()` alone. By hand it is `as: authenticated`, `when: "true"`.
- **Permissions.** `authorize('channels.delete')` and `authorize('messages.delete')` read a permission table and a JWT claim. By hand, as the spec does, they become `has_role(...)` rules.
- **`auth`, `system` name, the `inserted_at` default** (`auto: now`), and the roles for the auth hook.

## Policies, one by one

All 14 stayed `UNTRANSLATED`, each with its original SQL, so the draft can't be wider. Whether leaving each out was faithful (that is, nothing was lost or widened):

| Table | Policy | Original | Reason `infer` gave | What actually stopped it |
|---|---|---|---|---|
| `users`, `channels`, `messages` | Allow logged-in read access (3) | `select` using `auth.role() = 'authenticated'` | "a condition that isn't an owner check or `true`" | `auth.role()` is not in the translated list (docs/infer.md) |
| `users` | Allow individual insert, update (2) | `auth.uid() = id` | same | `id` is `uuid`, and a foreign key on the primary key stays a comment, so `id` is not a `user` field |
| `channels` | Allow individual insert, delete (2) | `auth.uid() = created_by` | same | `created_by` is a `ref`, not a `user` field |
| `messages` | Allow individual insert, update, delete (3) | `auth.uid() = user_id` | same | `user_id` is a `ref` |
| `channels`, `messages` | Allow authorized delete (2) | `authorize('...'::app_permission)` | same | a function call; `infer` does not read function bodies |
| `user_roles` | Allow auth admin to read user roles | `to supabase_auth_admin using (true)` | "roles supabase_auth_admin" | a role other than `anon` and `authenticated` |
| `user_roles` | Allow individual read access | `auth.uid() = user_id` | "a condition that isn't an owner check or `true`" | `user_id` is a `ref` |

12 of the 14 policies are the 12 of the hand-written spec, one to one (`compare.py` pairs them). The other two, on `user_roles`, are not modelled in the spec. A translated policy needs the owner column to be a `user` field. If the three owner columns referenced `auth.users` (an edit to a copy of the snapshot, to see the effect, not committed), `infer` translates 5 of 14: the two `channels` and three `messages` owner policies. The three reads would still not translate.

The draft's private entities differ from the live database in a way the comments say: the API reaches nothing in the draft, and everything in the database. That is the narrower direction. No translation widens, because none was written.

## Findings

Neither is a crash, a wrong translation or a wider draft; no change to `src/` was made.

1. **The reason text is the same for different causes.** For `auth.uid() = created_by`, `infer` says "a condition that isn't an owner check or `true`". It is an owner check; the cause is that the column is a `ref`, not a `user` field. A person reading the draft would look for the wrong thing. (`src/infer.rs`, `translate_policy`: one fixed string for every condition that fails to translate.)
2. **The `on delete` note is stale since #111.** `user_roles.user_id` has `on delete cascade` and the draft says "Svetra writes none", but `ref` now has `on_delete: cascade`. `infer` does not read the foreign key's action into `on_delete`, so a person adopting the draft loses the cascade unless they notice the comment. The `messages` to `channels` cascade is a comment for the other reason (`int` key).
3. **A small fidelity gap, not an access one:** the compiled `users.id` gets `default gen_random_uuid()` and no foreign key to `auth.users`; the database has neither. The table is private in the draft, so nobody can write it through the API.

## Limits

- One app, five tables, 14 policies. It shows what happens on a typical Supabase RLS idiom (`auth.role()`, `authorize()`, owner columns that reference `public.users`), not a rate for apps in general.
- "Faithful" here is by reading the SQL against `docs/infer.md`. Nothing was compiled back and compared to the original on a database, because nothing translated.
- The what-if (5 of 14) edits the snapshot by hand and is not a result of the app.

## Reproduce

```sh
cargo build
sh study/p7/outside/infer/reproduce.sh   # Docker, curl, python3 with PyYAML
python3 -I study/p7/outside/infer/compare.py
```

`reproduce.sh` fetches the two migrations at the commit, loads `src/standin/base.sql` and `extra.sql` and the migrations into one throwaway `postgres:16` container (removed on exit), takes the snapshot with `svetra infer --print-query` through `psql`, drafts, validates and compiles, and checks that the snapshot and draft equal the committed ones. `compare.py` prints the numbers above, and exits 1 if the draft exposes an entity with no policy.

| File | What |
|---|---|
| `snapshot.json` | the catalog snapshot, one JSON value |
| `draft.yaml` | `svetra infer --snapshot snapshot.json --out -` |
| `compare.py` | the comparison with `tests/fixtures/outside_slack.yaml` and the database's policies |
| `reproduce.sh` | everything from the migrations to the comparison |
