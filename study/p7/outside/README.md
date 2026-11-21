# An app written by someone else

The four apps in the study are the author's, and the generator was improved on them. This folder runs the same mutation study on one app Svetra's author did not write.

## The app

| | |
|---|---|
| App | Next.js Slack clone, an official Supabase example |
| Repository | `supabase/supabase`, path `examples/slack-clone/nextjs-slack-clone` |
| Commit | `9964968b53f078bf4e246fc25443e3220a8a143b` (master) |
| Licence | Apache-2.0 (`LICENSE` at the repository root, read at that commit) |
| Files used | `supabase/migrations/20240214102356_init.sql`, `20240214114147_auth-hook.sql` (and `seed.sql` for the role-permission rows) |

Why this one: it is Supabase's own example, so its policies are meant as a model and are not an accidental leak. It has 5 tables, ownership rules on `auth.uid()` (`user_id`, `created_by`, `id`), and a role table with a permission table behind it (`user_roles`, `role_permissions`, `authorize()`), which is the nearest thing to Svetra's roles. Only the files above were read, through the GitHub API at that commit; no live deployment was touched, and nobody was contacted.

## The spec

`tests/fixtures/outside_slack.yaml`, written by hand from the migrations. It validates with no errors (four SVT010 infos) and compiles. Its header lists every difference; the same list with reasons:

| The app | The spec | Why |
|---|---|---|
| `users.id` is the auth user id and the primary key | `Profile` has its own uuid `id` and a `user_id` of type `user` | The generated insert check inserts a row owned by the signed-in person, which already has a row under a `user`-typed key, so the unmutated app fails its own tests (duplicate key). The spec no longer limits a person to one profile |
| `messages.channel_id` references `channels` with `on delete cascade` | a plain `uuid`, no foreign key | Svetra can't write `on delete cascade`, and with a real foreign key the generated delete checks on `Channel` stop on the child rows (error 23503), so the baseline fails |
| bigint identity keys | uuid keys | A Svetra `ref` needs a uuid key |
| table `users` | entity `Profile` | A table is the snake_case of its entity name |
| roles in `user_roles`, copied into the JWT claim `user_role` by an auth hook; `role_permissions` maps role to permission; `authorize()` | `admin` and `moderator` as Svetra roles (rows in `svetra.role_grants`, checked live); the permissions written into the policies: `channels.delete` is `admin`, `messages.delete` is `admin` or `moderator` | Svetra has no permission table. The original's roles only change when a token is refreshed and a person holds one role in the claim; Svetra's are checked on every statement and a person may hold both |
| a person may read their own `user_roles` row | no such rule | `svetra.role_grants` is not an entity |
| `created_by`, `user_id` reference `public.users` | type `user` (reference `auth.users`) | The profile table isn't the key here |
| signup trigger, auth hook, realtime publication, `replica identity full` | not modelled | They are not access rules on the tables |

Everything else is one policy to one rule: read for every signed-in person on all three tables; insert, update and delete of own rows on `Message`; insert and delete of own rows on `Channel`; insert and update of own `Profile`; and the two deletes by role.

## Method

```sh
cargo build
python3 scripts/mutate.py --only outside_slack --out target/outside
python3 study/p7/classify.py study/p7/outside/results.json
python3 study/p7/outside/verify_slack.py --svetra target/debug/svetra study/p7/outside/results.json
python3 scripts/mutate.py --supabase --only outside_slack --out target/outside-supabase
```

`scripts/mutate.py` needed no change: `--only` takes any fixture name. Svetra 0.8.2 at the base commit of this branch; PostgreSQL 16 with the stand-in.

## Numbers

| | |
|---|---|
| Access checks generated | 244 (all pass on the unmutated app) |
| Mutants | 86 (18 of the 23 operators; the app has no holes, no scoped role and no `mfa` role, so the hole, scope and second-factor operators don't apply) |
| Killed | 69 |
| Survivors | 17: 16 equivalent, 1 masked, 0 real, 0 out of scope, 0 unclassified |
| Caught among mutants that change access | 69 of 69 (100%) |
| Raw kill rate | 80.2% (69 of 86) |
| Real Supabase (CLI 3.1.0) | identical outcomes and failing checks for all 86 mutants (`supabase/`) |

By direction: widen 36 of 53 caught, change 18 of 18, narrow 15 of 15; the 17 uncaught widening mutants are the survivors. `summary.md` has the per-operator table, `classification.md` the survivors with reasons.

The survivors:

- `policy-to-public` on the 9 write policies that need `auth.uid()` or a role (equivalent: `anon` never satisfies them).
- `policy-all-actions` on `messages_insert_own`, `messages_update_own` and `messages_delete_own` (equivalent: the added rule is the owner's, and a message's owner already has every action on it).
- `table-leak-read` on all three tables (equivalent: every select rule is already `true`).
- `role-ignore-scope` (equivalent: no scoped roles).
- `role-grants-writable` (masked: row-level security on `svetra.role_grants` with no rules still refuses every write; `role-grants-open`, which also turns it off, is killed).

`verify_slack.py` confirms the 17 on PostgreSQL 16: for the original and each survivor it runs 210 statements as six people (anon, three signed in without a role, an admin, a moderator) on rows they own and don't: select, insert as self and as another person, update, take a row over, give a row away, delete, and read and write the grants table. All 17 give the same outcome as the original in every cell. The same battery does tell the original from a killed mutant (checked on three).

## No real gap in access

Nothing the tests miss changes who can do what. The weakest spot of the generated tests is the same one as in the four apps: they cannot see a change that is equivalent, which is correct.

## Two limits of Svetra found by the app

Both made the generated tests fail on the unmutated app, so the spec was adjusted (table above). Neither is an access gap; both are things a reader of an outside app will hit.

1. **No `on delete cascade`.** A `ref` is always a plain foreign key, so a design with cascading deletes can't be written faithfully, and the generated delete checks on the parent run into the children.
2. **A primary key of type `user`.** The generated insert check for an owner rule on the key itself collides with the row the tests already seeded for that person.

## Limits of this study

- **The spec was written by Svetra's side.** It states, in Svetra's format, what the app's policies mean. A reviewer who disagrees with a reading would get different mutants. Policies were not guessed beyond the migrations, but the choices in the table above are mine.
- **One app, 86 mutants.** Small, and a simple one: three tables, one rule shape per action, global roles only. It exercises no scoped role, no hole, no update-with-check transition, and none of the operators that were the weak spots in the other apps. The 100% says the tests catch every access change here, not that they would on any app.
- **The spec is not the app.** The faithful parts are the policies. The tests check the generated database, a re-expression of the app, not the original migrations; the app's own roles mechanism (a JWT claim and a permission table) is replaced by Svetra's.
- **Classification by the same people.** The 16 equivalent and 1 masked survivors were judged by reading, and confirmed only by a finite battery, not by a proof.
- Mutants are the study's own 23 operators, not faults observed in this app.
