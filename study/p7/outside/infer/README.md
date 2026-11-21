# `svetra infer` on the Supabase Slack clone

`study/p7/outside/` ran the mutation study on Supabase's own Slack-clone example against a spec written by hand. This folder asks the other question: what does `svetra infer` (milestones 1 and 2, with profile tables and `auth.role()`) draft from the same app's database, and how does the draft compare with that hand-written spec and with the app's actual policies?

The app, commit and licence are the ones in [`../README.md`](../README.md) (`supabase/supabase`, `examples/slack-clone/nextjs-slack-clone`, commit `9964968b53f078bf4e246fc25443e3220a8a143b`, Apache-2.0). Only `supabase/migrations/` was read, with `curl` from `raw.githubusercontent.com` at that commit. No live deployment was touched and nobody was contacted.

## Result

| | |
|---|---|
| Tables in `public` | 5 (`users`, `channels`, `messages`, `user_roles`, `role_permissions`) |
| Entities drafted | 5 of 5 |
| Fields drafted | 18 of 18 columns |
| Things left as `UNTRANSLATED` | 11: 4 policies, 4 identity keys, 2 defaults, 1 foreign key |
| Policies translated | **10 of 14** (was 0 of 14 before profile owners and `auth.role()`) |
| Entities `private: true` | 2 of 5 (`RolePermissions`, which has no policy, and `UserRoles`, none of whose 2 translated) |
| Wider than the database | **none found.** The 4 policies left out only narrow the draft; the 10 translated ones were read against the SQL and `--check-draft` agreed on the entity it could check (below) |
| `svetra validate` on the draft | valid: 0 errors, 2 warnings (`SVT004`, `RolePermissions` and `UserRoles`, which nothing can write), 1 info `SVT009`, 3 info `SVT010` |
| `svetra compile` on the draft | succeeds with the same diagnostics |
| Against the hand-written spec | its 3 entities all have a counterpart; of its 13 fields, 6 identical, 2 same type but a modifier differs, 4 other type, 1 missing from the draft |

Svetra 0.8.2 on branch `infer-profile-owners` (on `develop` at `161d8da`, which has `--check-draft` and the user-typed primary key). Both migrations apply in order to PostgreSQL 16 with the stand-in (`svetra check --migrations` reports 2 applied).

### Differential check

`svetra infer --check-draft` compares the draft with a copy of the database, check by check. It skips an entity that has an untranslated policy (the draft is narrower there on purpose). To get evidence on `Channels`, the two `authorize()` delete policies were removed from a copy of the migrations and from the draft; `RolePermissions` and `Channels` then agree (4 and 38 checks, no disagreement). The other three entities were skipped, each for the tool's own reason:

- `Messages`: the database refuses the test rows, because the `channel_id` foreign key was left a comment (the key is `int`, Svetra has no int `ref`).
- `Users`: the test rows collide with the row the app's `handle_new_user` trigger creates for each person.
- `UserRoles`: a policy is untranslated.

So the check confirms 2 of 5 entities, not 5. For the rest, "not wider" rests on reading the SQL against `docs/infer.md`.

## What each table came out as

| Table | Entity and fields | Policies translated | Why not |
|---|---|---|---|
| `users` | `Users`: `id` is `{type: user, primary: true}` (a profile table), `username` text, `status` enum | 3 of 3 | |
| `channels` | `Channels`: `id` int primary, `inserted_at`, `slug` unique, `created_by` `user` (lossy note: the key now goes to `auth.users`) | 3 of 4 | delete by permission is `authorize('channels.delete')`, a function call |
| `messages` | `Messages`: `id` int primary, `inserted_at`, `message`, `user_id` `user`, `channel_id` int | 4 of 5 | delete by permission is `authorize('messages.delete')`; the `channel_id` foreign key with `on delete cascade` stays a comment (int key) |
| `user_roles` | `UserRoles`: `id`, `user_id` `user`, `role` enum, unique `[user_id, role]` | 0 of 2 | one policy is `to supabase_auth_admin`; the other reads `auth.uid() = user_id`, but the database withholds the table privilege from its role (`public`), so it is dropped |
| `role_permissions` | `RolePermissions`: `id`, `role`, `permission`, unique | 0 of 0 | row-level security on with no policy: `private: true` |

## Entities and fields against the hand-written spec

The hand-written spec changed the app on purpose (listed in `../README.md`) so that the generated tests could run. The draft keeps the app as it is, so each difference below is the draft being truer to the database than the spec is.

| Table | Draft against the spec | Fields |
|---|---|---|
| `users` / `Profile` | the draft's `id` is the `user` primary key; the spec has its own uuid `id` and a separate `user_id` | `username`, `status` identical; `id` other type; `user_id` only in the spec |
| `channels` / `Channel` | the spec's key is uuid, the draft's is `int` (the bigint identity); `created_by` is now `user` in both | `slug`, `created_by` identical; `inserted_at` same type, but the spec has `auto: now` and the draft leaves the default as a comment; `id` other type |
| `messages` / `Message` | same, and `channel_id` is `int` in the draft, uuid in the spec | `message`, `user_id` identical; `inserted_at` as above; `id`, `channel_id` other type |
| `user_roles`, `role_permissions` | not entities in the spec (its roles are Svetra's own) | both drafted |

A person adopting the draft would still have to decide, by hand:

- **Keys.** Svetra has no auto-increment and a `ref` needs a uuid key, so the four bigint identity keys and `messages.channel_id` stay `int`, and the `messages` to `channels` foreign key is a comment. They would change them to uuid, as the hand spec does.
- **Permissions.** `authorize('channels.delete')` and `authorize('messages.delete')` read a permission table and a JWT claim. By hand, as the spec does, they become `has_role(...)` rules.
- **`UserRoles`.** The auth-hook policy and the read-own policy (whose table privilege the database withholds from `public`).
- **`auth`, `system` name, the `inserted_at` default** (`auto: now`), and the roles for the auth hook.

## Policies, one by one

| Table | Policy | Original | Draft |
|---|---|---|---|
| `users`, `channels`, `messages` | Allow logged-in read access (3) | `select` using `auth.role() = 'authenticated'` | translated: `as: authenticated`, `when: "true"` |
| `users` | Allow individual insert, update (2) | `auth.uid() = id` | translated: `id == auth.user` |
| `channels` | Allow individual insert, delete (2) | `auth.uid() = created_by` | translated |
| `messages` | Allow individual insert, update, delete (3) | `auth.uid() = user_id` | translated |
| `channels`, `messages` | Allow authorized delete (2) | `authorize('...'::app_permission)` | left: "calls the function authorize(); infer doesn't read function bodies" |
| `user_roles` | Allow auth admin to read user roles | `to supabase_auth_admin using (true)` | left: "roles supabase_auth_admin" |
| `user_roles` | Allow individual read access | `auth.uid() = user_id` | left: "the database withholds the table privilege from this role" |

12 of the 14 policies are the 12 of the hand-written spec, one to one (`compare.py` pairs them); 10 of those 12 translate. The other two, on `user_roles`, are not modelled in the spec.

The owner policies are `to public`, so the draft says `as: anyone` with `id == auth.user` or similar. A request with no user has a null `auth.uid()`, so it matches nothing, in the database and in the draft.

## Findings

Both findings of the first version of this study are fixed by the branch.

1. **Reason text.** Each untranslated policy now names its real cause ("calls the function authorize()", "roles supabase_auth_admin", and so on), not one fixed sentence.
2. **`on delete`.** A foreign key's `on delete cascade` now becomes `on_delete: cascade` on a `ref`. In this app the one cascade that lands on a `user` field is `user_roles.user_id`, and a `user` field has no `on_delete`, so the draft keeps it in the `# lossy` note (see `UserRoles.user_id` in `draft.yaml`). The `messages` to `channels` cascade is still a comment, because the key is `int`.
3. **A small fidelity gap, not an access one:** a column that referenced `public.users` now references `auth.users`, which accepts a person with no row in `users`. The draft says so in a `# lossy` note.

## Limits

- One app, five tables, 14 policies. It shows what happens on a typical Supabase RLS idiom (`auth.role()`, `authorize()`, owner columns that reference a profile table), not a rate for apps in general.
- The differential check confirmed 2 of 5 entities (above). The rest rest on reading the SQL.
- The caveat in `docs/infer.md` applies: a request with no JWT as `anon` has a null `auth.role()`. None of this app's policies names `anon`.

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
