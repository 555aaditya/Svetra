# P7 study data

Data for "Checking Row-Level Security Against Its Specification: Spec-Derived Access Tests and a Mutation Study". Every mutant is in `results.json` with its SQL, outcome and failing checks; `summary.md` has the kill rates.

| Folder | Tests of | Server |
|---|---|---|
| `round1/` | Svetra 0.4.0 | PostgreSQL 16 with the Supabase stand-in |
| `round2/` | Svetra 0.4.1 | the same |
| `round1-supabase/` | 0.4.0's generator | real local Supabase (CLI 3.1.0, PostgreSQL 17) |
| `round2-supabase/` | 0.4.1's generator | the same |
| `round3/` | the generator after #89 (an owner who also holds a role, and an owner and a reference changed together) | PostgreSQL 16 with the Supabase stand-in |
| `round3-supabase/` | the same | real local Supabase (CLI 3.1.0, PostgreSQL 17), no patch needed |

The Supabase rounds use the 0.4.0 and 0.4.1 sources with only the 0.4.2 revocation fix applied (`revoke ... from public, anon, authenticated` in `src/compile/sql.rs`), because the released compilers fail their own tests on real Supabase. Their outcomes and failing checks are identical to the stand-in's.

`classify.py` sorts each surviving mutant into equivalent, masked, out of scope or real, with the reason; `classification.md` is its output for the three stand-in rounds.

`z3/` holds a Z3 model of the bookings policies that checked the classification, and a script that confirms its findings on PostgreSQL 16 (`z3/README.md`). `operators-and-real-faults.md` compares the 23 operators with faults found in the leak study (`eval/leakstudy`).

**A correction.** The first classification judged four bookings survivors masked or equivalent. The Z3 model disagreed, and each was confirmed real on PostgreSQL 16: bookings has two update policies, and loosening one lets the other's people through. `classify.py` and `classification.md` here are the corrected ones: round 1 has 13 real gaps and round 2 has 4, not 9 and 0. Round 3 closes the fifth class: 147 of 147 mutants that change access are caught, and the survivors are 44 equivalent, 5 masked and 1 out of scope. Round 3 on real Supabase has identical outcomes and failing checks. Rounds 1 and 2 are not an unbiased kill rate, and neither is round 3: the generator was improved on the same four apps.

## Reproduce

From a checkout of each tag (`v0.4.0` for round one, `v0.4.1` for round two), after `cargo build`:

```sh
python3 scripts/mutate.py --out target/round            # stand-in; needs Docker
python3 scripts/mutate.py --supabase --out target/round # real Supabase; needs Docker and Node
python3 study/p7/classify.py round1/results.json round2/results.json
```

For the Supabase runs, first apply the two `revoke` changes above to the old source. Use `scripts/mutate.py` from this branch: the tags' own copy has no `--supabase`.
