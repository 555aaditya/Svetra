# Mutation study: summary

Killed: at least one access check failed. Error: the test file stopped with an error (also detected). Stillborn: the mutation itself didn't apply, so it's left out.

| App | Checks | Mutants | Killed | Error | Survived | Stillborn | Kill rate |
|---|---|---|---|---|---|---|---|
| finledger | 89 | 30 | 27 | 0 | 3 | 0 | 90.0% |
| posts | 61 | 42 | 20 | 0 | 22 | 0 | 47.6% |
| bookings | 178 | 83 | 68 | 0 | 15 | 0 | 81.9% |
| clinic | 74 | 42 | 32 | 0 | 10 | 0 | 76.2% |

| Direction | Live mutants | Caught | Kill rate |
|---|---|---|---|
| widen | 124 | 79 | 63.7% |
| change | 40 | 38 | 95.0% |
| narrow | 33 | 30 | 90.9% |

| Operator | Direction | Live | Caught | Kill rate |
|---|---|---|---|---|
| hole-body-callable | widen | 2 | 2 | 100.0% |
| hole-extra-grant | widen | 2 | 1 | 50.0% |
| hole-no-guard | widen | 1 | 1 | 100.0% |
| hole-wrapper-anon | widen | 2 | 2 | 100.0% |
| policy-all-actions | widen | 23 | 19 | 82.6% |
| policy-check-true | widen | 9 | 7 | 77.8% |
| policy-drop | narrow | 26 | 24 | 92.3% |
| policy-role-swap | change | 22 | 21 | 95.5% |
| policy-to-public | widen | 26 | 2 | 7.7% |
| policy-using-negate | change | 18 | 17 | 94.4% |
| policy-using-true | widen | 18 | 15 | 83.3% |
| role-check-true | widen | 2 | 2 | 100.0% |
| role-grants-open | widen | 2 | 2 | 100.0% |
| role-grants-writable | widen | 2 | 0 | 0.0% |
| role-ignore-person | widen | 2 | 2 | 100.0% |
| role-ignore-role | widen | 2 | 0 | 0.0% |
| role-ignore-scope | widen | 2 | 1 | 50.0% |
| role-ignore-second-factor | widen | 1 | 1 | 100.0% |
| table-leak-read | widen | 7 | 5 | 71.4% |
| table-leak-read-anon | widen | 7 | 5 | 71.4% |
| table-leak-write | widen | 7 | 6 | 85.7% |
| table-revoke-read | narrow | 7 | 6 | 85.7% |
| table-rls-off | widen | 7 | 6 | 85.7% |

## Survivors

- **finledger** `policy-to-public` on `account.owner_reads_own_accounts`
- **finledger** `policy-to-public` on `transaction.owner_reads_own_transactions`
- **finledger** `hole-extra-grant` on `transfer`
- **posts** `policy-drop` on `audit_log.svetra_hole_write_audit`
- **posts** `policy-to-public` on `audit_log.svetra_hole_write_audit`
- **posts** `policy-to-public` on `post.posts_by_author_delete`
- **posts** `policy-all-actions` on `post.posts_by_author_delete`
- **posts** `policy-to-public` on `post.posts_by_author_insert`
- **posts** `policy-all-actions` on `post.posts_by_author_insert`
- **posts** `policy-drop` on `post.posts_by_author_select`
- **posts** `policy-using-true` on `post.posts_by_author_select`
- **posts** `policy-using-negate` on `post.posts_by_author_select`
- **posts** `policy-to-public` on `post.posts_by_author_select`
- **posts** `policy-role-swap` on `post.posts_by_author_select`
- **posts** `policy-all-actions` on `post.posts_by_author_select`
- **posts** `policy-to-public` on `post.posts_by_author_update`
- **posts** `policy-all-actions` on `post.posts_by_author_update`
- **posts** `policy-to-public` on `post.public_read`
- **posts** `table-rls-off` on `audit_log`
- **posts** `table-leak-read` on `audit_log`
- **posts** `table-leak-read-anon` on `audit_log`
- **posts** `table-leak-write` on `audit_log`
- **posts** `table-revoke-read` on `audit_log`
- **posts** `table-leak-read` on `post`
- **posts** `table-leak-read-anon` on `post`
- **bookings** `policy-to-public` on `booking.admins_manage_all_delete`
- **bookings** `policy-to-public` on `booking.admins_manage_all_select`
- **bookings** `policy-to-public` on `booking.admins_manage_all_update`
- **bookings** `policy-to-public` on `booking.customers_see_own_insert`
- **bookings** `policy-to-public` on `booking.customers_see_own_select`
- **bookings** `policy-to-public` on `booking.staff_manage_local_select`
- **bookings** `policy-to-public` on `booking.staff_manage_local_update`
- **bookings** `policy-to-public` on `location.admins_see_locations_insert`
- **bookings** `policy-to-public` on `location.admins_see_locations_select`
- **bookings** `policy-using-true` on `location.admins_see_locations_update`
- **bookings** `policy-check-true` on `location.admins_see_locations_update`
- **bookings** `policy-to-public` on `location.admins_see_locations_update`
- **bookings** `policy-to-public` on `location.staff_see_location`
- **bookings** `role-ignore-role` on `svetra.has_role`
- **bookings** `role-grants-writable` on `svetra.role_grants`
- **clinic** `policy-to-public` on `visit.admins_manage_delete`
- **clinic** `policy-to-public` on `visit.admins_manage_select`
- **clinic** `policy-using-true` on `visit.admins_manage_update`
- **clinic** `policy-check-true` on `visit.admins_manage_update`
- **clinic** `policy-to-public` on `visit.admins_manage_update`
- **clinic** `policy-to-public` on `visit.patients_see_own_insert`
- **clinic** `policy-to-public` on `visit.patients_see_own_select`
- **clinic** `role-ignore-role` on `svetra.has_role`
- **clinic** `role-ignore-scope` on `svetra.has_role`
- **clinic** `role-grants-writable` on `svetra.role_grants`
