# Mutation study: summary

Killed: at least one access check failed. Error: the test file stopped with an error (also detected). Stillborn: the mutation itself didn't apply, so it's left out.

| App | Checks | Mutants | Killed | Error | Survived | Stillborn | Kill rate |
|---|---|---|---|---|---|---|---|
| outside_slack | 244 | 86 | 69 | 0 | 17 | 0 | 80.2% |

| Direction | Live mutants | Caught | Kill rate |
|---|---|---|---|
| widen | 53 | 36 | 67.9% |
| change | 18 | 18 | 100.0% |
| narrow | 15 | 15 | 100.0% |

| Operator | Direction | Live | Caught | Kill rate |
|---|---|---|---|---|
| policy-all-actions | widen | 12 | 9 | 75.0% |
| policy-check-true | widen | 5 | 5 | 100.0% |
| policy-drop | narrow | 12 | 12 | 100.0% |
| policy-role-swap | change | 12 | 12 | 100.0% |
| policy-to-public | widen | 12 | 3 | 25.0% |
| policy-using-negate | change | 6 | 6 | 100.0% |
| policy-using-true | widen | 6 | 6 | 100.0% |
| role-check-true | widen | 1 | 1 | 100.0% |
| role-grants-open | widen | 1 | 1 | 100.0% |
| role-grants-writable | widen | 1 | 0 | 0.0% |
| role-ignore-person | widen | 1 | 1 | 100.0% |
| role-ignore-role | widen | 1 | 1 | 100.0% |
| role-ignore-scope | widen | 1 | 0 | 0.0% |
| table-leak-read | widen | 3 | 0 | 0.0% |
| table-leak-read-anon | widen | 3 | 3 | 100.0% |
| table-leak-write | widen | 3 | 3 | 100.0% |
| table-revoke-read | narrow | 3 | 3 | 100.0% |
| table-rls-off | widen | 3 | 3 | 100.0% |

## Survivors

- **outside_slack** `policy-to-public` on `channel.channels_delete_admin`
- **outside_slack** `policy-to-public` on `channel.channels_delete_own`
- **outside_slack** `policy-to-public` on `channel.channels_insert_own`
- **outside_slack** `policy-to-public` on `message.messages_delete_own`
- **outside_slack** `policy-all-actions` on `message.messages_delete_own`
- **outside_slack** `policy-to-public` on `message.messages_delete_staff`
- **outside_slack** `policy-to-public` on `message.messages_insert_own`
- **outside_slack** `policy-all-actions` on `message.messages_insert_own`
- **outside_slack** `policy-to-public` on `message.messages_update_own`
- **outside_slack** `policy-all-actions` on `message.messages_update_own`
- **outside_slack** `policy-to-public` on `profile.profiles_insert_own`
- **outside_slack** `policy-to-public` on `profile.profiles_update_own`
- **outside_slack** `table-leak-read` on `channel`
- **outside_slack** `table-leak-read` on `message`
- **outside_slack** `table-leak-read` on `profile`
- **outside_slack** `role-ignore-scope` on `svetra.has_role`
- **outside_slack** `role-grants-writable` on `svetra.role_grants`
