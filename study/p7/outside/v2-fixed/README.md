# The second outside-app run, after the generator fix

The v2 run (`../v2/`) found a real gap in the generated tests: for an entity keyed by a `user` field, a person with no profile could take over someone else's profile by changing its key, and a person could give theirs away, and no check noticed (`using (true)` and `with check (true)` on `profile.profiles_update_own`). PR #120 closed it: the change-who-owns checks on a user-keyed entity now also try keys nobody holds. This folder is the same 86 mutants of the same fixture (`tests/fixtures/outside_slack_v2.yaml`), run again with the generator after #120, on the stand-in and on a real local Supabase.

| | v2 (before the fix) | v2-fixed (after) |
|---|---|---|
| Access checks | 258 | 278 |
| Mutants | 86 | 86 |
| Killed | 67 | 69 |
| Survivors | 19 | 17 |
| Raw kill rate | 77.9% | 80.2% |
| Survivors: equivalent / masked / real | 16 / 1 / 2 | 16 / 1 / 0 |
| Caught among mutants that change access | 67 of 69 (97.1%) | **69 of 69 (100%)** |

Exactly two mutants changed, from survived to killed: `policy-using-true` and `policy-check-true` on `profile.profiles_update_own`. Nothing killed before survives now, and the other 17 survivors are the ones `../v2/classification.md` judged equivalent or masked, unchanged.

Real Supabase (CLI 3.1.0, PostgreSQL 17, `supabase/`): the same outcome and the same failing checks for all 86 mutants as on the stand-in. `summary.md` has the per-operator table and `classification.md` the survivors.

**Limits.** The same as the first run: the specification was written by Svetra's side from the app's policies, one simple application, and the generator was improved after seeing this application's result, so 69 of 69 shows the gap is closed, not an unbiased kill rate. The equivalent and masked survivors are still confirmed by reading and by the finite battery in `../verify_slack_v2.py`, not by proof.

To reproduce, from the repository root after `cargo build`, with Docker: `python3 scripts/mutate.py --only outside_slack_v2 --out target/outside-v2-fixed` (and `--supabase` for the second).
