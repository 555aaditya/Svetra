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
