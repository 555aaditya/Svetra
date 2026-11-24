### svetra plan: 8 changes to Shop (from base:svetra.yaml)

| | Change |
|---|---|
| destructive | remove Comment: every comment is deleted |
| rename | rename Booking.customer to client: every value is kept |
| safe | add rule all_bookings |
| safe | remove rule own_bookings |
| safe | remove rule all_comments |
| wider | A signed-in person can now see all bookings (was: some).<br>because: Signed-in people can see bookings. |
| wider | Another signed-in person can now see all bookings (was: some).<br>because: Signed-in people can see bookings. |
| wider | A signed-in person who owns nothing can now see all bookings (was: none).<br>because: Signed-in people can see bookings. |

**Needs a yes:** `remove_entity`, `wider_access`. A person who owns the data has to agree, then add them to `accept` in the workflow.

Declared renames need a migration before `supabase db diff`; run `svetra plan` locally to write it.
