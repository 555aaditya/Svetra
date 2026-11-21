# Z3 spike on the bookings policies (Arun Sharma's suggestion)

Arun Sharma suggested checking the specification with Z3 or Lean 4 (2026-11-19). This is the smallest useful test of that idea, and it turned out to check the study itself.

**What it does.** `bookings_spike.py` reads the bookings application's policies as formulas over three facts about a person (owns the row, is staff at its location, is admin; anon has none of them), applies each of the study's seven policy operators to the formulas, and asks Z3 whether the mutant gives anyone access the original does not. It then compares Z3's answer with what the tests found when they ran against the real mutant. No database is involved.

```sh
pip install z3-solver
python3 bookings_spike.py ../round1/results.json ../round2/results.json
```

**Scope.** Bookings only, the seven policy operators, 67 mutants. Every condition here is a boolean over those facts, so Z3 is more machinery than the problem needs; what it shows is the pipeline (policy text to formula to counterexample), which also handles richer conditions. Not modelled: table privileges.

**What it found.**

| Z3's verdict | Tests | Mutants |
|---|---|---|
| no change in access | survived | 13 |
| widens access | survived | **4** |
| widens, narrows or changes access | killed | 50 |

The 13 confirm the "equivalent" and "masked" calls for those mutants (the study had them judged by one person; this is a second, independent check). The 4 are a disagreement: the study had classified them as equivalent (1) or masked (3).

**The four are real gaps, verified on PostgreSQL 16** (`verify_bookings.py`, any machine with Docker): bookings has two update policies, one for staff and one for admins. PostgreSQL combines the USING conditions of the update policies with OR and, separately, their WITH CHECK conditions with OR. So loosening one policy's condition lets the other policy's person through:

- `policy-check-true` on either update policy: **staff at one location can take a booking**: move it to another location and make themselves its customer. Denied by the original, allowed by the mutant.
- `policy-using-true` on either update policy: **a customer who is also staff at another location can move their own booking there**. Denied by the original, allowed by the mutant.

The generated tests did not catch any of the four in either round, because their small world has no person with that combination and no update that changes two fields to reach it. The classifier had treated an update policy's WITH CHECK as the same rule as its USING, which holds only when a table has one update policy.

**Consequence.** `classify.py` now classifies those four as real, and `classification.md` is regenerated. Round one: 13 real gaps, not 9 (equivalent 44, masked 6). Round two: 4 real gaps remain, not 0 (equivalent 44, masked 5). The tests caught 133 of 146 mutants that change access in round one (91.1%) and 143 of 147 in round two (97.3%), not 133 of 142 and 143 of 143. The other apps have one update policy per table, so their equivalent and masked calls stand (checked 2026-11-20).

**Limits.** One application, one person's reading of which atoms matter, and a model that needed one correction from observed PostgreSQL behaviour (an update also needs the select policies to show the row, before and after). The spike does not replace running the tests; it checks the classification and finds worlds the tests lack.

# The model on all four apps (`policies.py`)

`policies.py` generalises the spike to finledger, posts and clinic, and re-does bookings with the same code. Unlike the spike, nothing is copied by hand: it runs `svetra compile` on each fixture and reads the policies, the table privileges and the role definition out of the generated SQL, so it reads what is deployed.

```sh
cargo build
python3 policies.py --svetra ../../../target/debug/svetra ../round1/results.json ../round2/results.json ../round3/results.json
python3 verify_policies.py --svetra ../../../target/debug/svetra --repo ../../.. ../round1/results.json   # Docker
```

**Facts it models.** For a person and a row: owns a user column (`author == auth.user`); owns the row a reference points to (`source.owner == auth.user`, finledger); holds a global role, with a second factor (`aal2`) when the role has `mfa: true` (clinic's admin); holds a role scoped to the row's column (`has_role('staff', location)`). An update has two rows, before and after. Conditions use SQL's three-valued logic: for anon and for a hole's database role `auth.uid()` is NULL, so `column = auth.uid()` is NULL, `NOT NULL` is NULL and a row is not shown, while `has_role()` is a plain false and its negation is true. Table privileges are read from the SQL (`revoke all` on private tables; default grants otherwise; each hole role's own grants). The actors are anon, signed-in, and each hole role. The seven policy operators are applied as `scripts/mutate.py` writes them. The verdicts are `equal`, `widen`, `narrow`, `change`, and `hole-only` (only a hole's own role changes, which the tests do not run). Bookings gets the same verdict as `bookings_spike.py` for all 201 bookings mutants in the three rounds.

**One rule added to the spike's.** A delete also needs the select policies to show the row, because a delete finds its row with a WHERE. It changes no bookings verdict.

**Agreement** (policy-level mutants; `stillborn` excluded; no mutant was "not modelled"):

| Round | App | Mutants | Model agrees with tests | Tests miss a change the model sees | Survivors classified the same way |
|---|---|---|---|---|---|
| 1 | finledger | 16 | 15 | 1 | 3 of 3 |
| 1 | posts | 29 | 27 | 2 | 17 of 17 |
| 1 | clinic | 30 | 30 | 0 | 7 of 7 |
| 1 | bookings | 67 | 63 | 4 | 17 of 17 |
| 2 | finledger | 16 | 16 | 0 | 2 of 2 |
| 2 | posts | 29 | 29 | 0 | 15 of 15 |
| 2 | clinic | 30 | 30 | 0 | 7 of 7 |
| 2 | bookings | 67 | 63 | 4 | 17 of 17 |
| 3 | finledger | 16 | 16 | 0 | 2 of 2 |
| 3 | posts | 29 | 29 | 0 | 15 of 15 |
| 3 | clinic | 30 | 30 | 0 | 7 of 7 |
| 3 | bookings | 67 | 67 | 0 | 13 of 13 |

"Agrees with tests" means: the model sees a change and the tests caught the mutant, or it sees none and the mutant survived (a `hole-only` verdict agrees with either). "Survivors classified the same way": the model says `equal` for the mutants `classify.py` calls equivalent or masked, `hole-only` for out-of-scope, and a widening for real.

**Findings.**

1. **No disagreement with `classify.py` on any of the 12 round-and-app cells.** Every survivor of finledger, posts and clinic that is called equivalent, masked or out of scope is one the model also finds changes nothing for anon and signed-in people (the one out-of-scope survivor, posts' `policy-drop` of the hole's own policy, changes only the hole's own role); every one called real is a widening in the model. This is a second reader agreeing, not a proof: both readers are encodings of the same PostgreSQL rules.
2. **The model sees three more places where the tests missed a change, all already classified real:** round 1 only, finledger `policy-using-true` on `transaction.owner_reads_own_transactions` (any signed-in person reads every transaction), and posts `policy-using-true` and `policy-check-true` on `post.posts_by_author_update` (a person takes over a post, or gives theirs away, by changing its author). Rounds 2 and 3 close all three. The other four tests-miss rows are the bookings update-policy gaps from the spike.
3. **No mutant the tests caught is one the model calls unchanged.**

**Checked on PostgreSQL 16** (`verify_policies.py`, seeded world, the mutant's exact SQL from `round1/results.json`): the three round-1 survivors in finding 2 each go from refused to allowed (B takes over A's post: 0 rows then 1; A gives a post to B: row-level security error then 1; a person with no account reads the transaction: 0 rows then 1). The hole's own role inserts into `audit_log` before and is refused after dropping its policy, so the out-of-scope call is a real change to the hole, as classified. Three survivors the model calls unchanged hold on PostgreSQL too: clinic `policy-using-true` and `policy-check-true` on `admins_manage_update` (a patient cannot edit or give away their visit: 0 rows or refused, before and after), and finledger `policy-to-public` on `owner_reads_own_accounts` (anon sees 0 accounts before and after). No disagreement had to be settled, so the model needed no fix; the delete rule above and the three-valued logic were added while writing it and checked against bookings' known results.

**Limits.**

- Only the seven policy operators, 75 non-bookings mutants a round. The 16 role-level, hole-level and table-level operators are not modelled, so their equivalent and masked calls (`role-grants-writable`, `role-ignore-role`, `role-ignore-scope`, `hole-extra-grant`, `hole-wrapper-anon`, `table-leak-*` on posts) have no second reader here.
- The condition reader recognises the four shapes above and raises `Unmodelled` for any other, so a new fixture with a richer condition (a comparison, a text column, a function) is reported "not modelled" rather than guessed. All 12 cells have none.
- Facts are independent: the model does not know unique or foreign-key constraints, that old and new column values may be equal, or that a referenced row must exist, so a counterexample is a candidate world, and the tests-miss ones were checked on PostgreSQL. Table privileges are the defaults the compiler's SQL implies, not read from a database.
- Four fixtures. The PostgreSQL scenarios are one per mutant the model suggests, not an exhaustive search.
