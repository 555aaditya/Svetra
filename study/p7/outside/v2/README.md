# The outside app again, without the two workarounds

PR #115 made the generated tests work for entities keyed by `user`, and the first outside run had needed two workarounds in the spec for the limits it found. `tests/fixtures/outside_slack_v2.yaml` removes both and runs the same study. `outside_slack.yaml` and the first run (`../`) are unchanged.

## What changed in the spec

| | `outside_slack` | `outside_slack_v2` |
|---|---|---|
| `Profile` key | own uuid `id` plus a `user_id` of type `user` | `id` is `type: user, primary: true`; policies are `id == auth.uid` |
| `Message.channel_id` | plain `uuid`, no foreign key | `ref -> Channel, on_delete: cascade` |

`svetra validate`: 0 warnings (only SVT010 infos).

## Method

```sh
cargo build
python3 scripts/mutate.py --only outside_slack_v2 --out target/outside-v2
python3 study/p7/classify.py study/p7/outside/v2/results.json
python3 study/p7/outside/verify_slack_v2.py --svetra target/debug/svetra study/p7/outside/v2/results.json
python3 scripts/mutate.py --supabase --only outside_slack_v2 --out target/outside-v2-supabase
```

`verify_slack_v2.py` is `verify_slack.py` adapted to the user-keyed Profile, with two more cells: update the key of a profile to a person who has none, and the same for a person without a profile (carol).

## Numbers

| | first run | v2 |
|---|---|---|
| Access checks | 244 | 258 |
| Mutants | 86 | 86 |
| Killed | 69 | 67 |
| Survivors | 17 | 19 |
| Raw kill rate | 80.2% | 77.9% |
| Widen caught | 36 of 53 | 34 of 53 |
| Change caught | 18 of 18 | 18 of 18 |
| Narrow caught | 15 of 15 | 15 of 15 |
| Survivors: equivalent / masked / real | 16 / 1 / 0 | 16 / 1 / 2 |
| Caught among mutants that change access | 69 of 69 | 67 of 69 (97.1%) |

Real Supabase (CLI, PostgreSQL 17): the same outcome and the same failing checks for all 86 mutants (`supabase/`). `summary.md` has the per-operator table and `classification.md` the survivors.

## A real gap, in the generated tests

The two new survivors are `policy-using-true` and `policy-check-true` on `profile.profiles_update_own`. They are real, not equivalent or masked. On PostgreSQL 16, `verify_slack_v2.py` finds 14 cells where they differ from the original, for example:

- carol, who has no profile, takes over alice's profile by changing its key to her own id: original denied, mutant allowed;
- alice gives her profile to a person who has none: original denied, mutant allowed.

The other 17 survivors behave exactly like the original in all 228 cells of the battery.

Why the tests miss it: with a `user`-typed primary key, the generated "change who owns" checks can only aim at the other seeded people, and their rows already exist. The statement fails with a duplicate-key error whether or not the policy allows the change, so it cannot tell the two apart. No check tries a key nobody holds yet.

This is a limit of the generated tests for entities keyed by `user`, which the design of PR #115 introduces. It is not a bug in the app. It is not fixed here: this is a study, and the generator change should be its own issue and PR.

## Remaining differences from the original app

1. Row ids of `Channel` and `Message` are uuid, not bigint identity.
2. The table is `profile` (entity `Profile`), not `users`.
3. Roles are Svetra's own (`svetra.role_grants`, checked live, a person may hold both); the two permissions are written into the policies; no `role_permissions`, `authorize()` or auth hook.
4. `Channel.created_by` and `Message.user_id` are type `user` (reference `auth.users`), not `public.users`. A scratch copy with `created_by` as `ref -> Profile` validated with 0 warnings; I did not test that it compiles or passes its generated tests.
5. A person cannot read their own `user_roles` row.
6. The signup trigger, realtime publication and `replica identity full` are not modelled.

## Decisions

- A new `verify_slack_v2.py` instead of extending the first script, so the first run stays reproducible.
- Everything not named above is kept identical to `outside_slack`, including item 4.
- `classify.py` gets rules scoped to `outside_slack_v2`; the four-app and first-run outputs are byte-identical to before.
