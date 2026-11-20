# The 23 operators against real faults

Habib's third point (2026-11-20): the operators are not tied to real faults. This compares them with what `svetra check` found in the migrations of 300 randomly drawn public Supabase repositories (the leak study, `eval/leakstudy` in Svetra 0.8.1; 263 repositories could be checked; first run 2026-11-18, re-run after the check fix of 2026-11-19). Counts are repositories with at least one finding of that kind. Aggregates only: no repository, owner or table is named.

**Read this first.** Migrations are not live applications. A match means the operator changes the database in the same shape as a fault found in real code, not that the fault is common in deployed apps, and not that the operator was derived from it. Two hand audits of the study (30 findings each) support the findings below.

| Operator group | Real counterpart found in the study | Repositories (of 263) | Match |
|---|---|---|---|
| `table-rls-off` | Row-level security off on a table the API can reach (`rls_off`, critical). Also the failure recorded in CVE-2025-48757 | 46 | Direct |
| `policy-using-true` | A policy for update, delete or all commands whose condition is `true`, open to anon or public (`open_write`, critical) | 37 | Direct |
| `policy-check-true` | An insert policy with a check of `true` (`open_insert`, warning); often a contact form, sometimes not | 63 | Direct, severity depends on intent |
| `policy-to-public` | A policy meant for the backend ("Service role manages ...", "System can ...") with no `to` clause, so it applies to every role. Six of the 11 open writes in the second audit | 6 of 11 audited | Direct, and the commonest real open write we saw |
| `policy-all-actions` | `for all` policies on a table where only some commands were meant ("Allow all access" with `using (true)`) | seen in the audits, not counted | Same shape, uncounted |
| `table-leak-read`, `table-leak-read-anon` | `select` policies with `true` (`open_read`, warning); public data or a leak depending on the table | 112 | Direct, intent-dependent |
| `table-leak-write` | A permissive policy for every command with `true` for signed-in people (`signed_in_write`) | 25 | Direct |
| `hole-wrapper-anon`, `hole-body-callable`, `hole-extra-grant` | A `SECURITY DEFINER` function the API can call (`definer_function`); Section 7's finding that Supabase grants `anon` execute on new functions | 204 | Same class (callable privileged function); the hole operators act on Svetra's generated functions |
| `policy-drop`, `table-revoke-read` | Row-level security on with no policy, so nobody gets access (`no_policy`, a note) | 87 | Narrowing; breaks features, not a leak |
| `policy-using-negate`, `policy-role-swap` | Nothing seen | n/a | Plausible by-hand edits; no evidence either way |
| `role-check-true`, `role-ignore-person`, `role-ignore-role`, `role-ignore-scope`, `role-ignore-second-factor`, `role-grants-writable`, `role-grants-open` | Nothing: the study does not read custom role tables or `has_role`-style functions | n/a | No evidence either way |

**Faults the study found that have no operator:**

| Fault | Repositories (of 263) | Note |
|---|---|---|
| A view readable through the API without `security_invoker` (`view_bypass`; one critical, because it exposed `auth.users`) | 38 | The clearest gap: add an operator that removes `security_invoker` from a view |
| `SECURITY DEFINER` function without a fixed `search_path` (`mutable_search_path`) | 66 | A function-body fault outside the policy operators |
| A policy that trusts `user_metadata` (`user_metadata`) | 2 | Rare here; easy to add as an operator |

**What this gives the paper (Construct threats):** the policy, table and function-privilege operators have counterparts in real code; the role-table operators do not, and three fault kinds have no operator. It says the operators are plausible, not that they are frequent. Do not write "the operators cover N% of real faults": the study only sees what `check` looks for.

**Possible follow-ups:** add operators for `view_bypass` and `user_metadata` and re-run the study (the harness and mutate.py already make this cheap); read the role-table question as "does anyone write `has_role`-style helpers in public repos", which the leak study could count.
