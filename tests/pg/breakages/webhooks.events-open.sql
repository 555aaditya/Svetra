-- The handled-events table readable by signed-in people: anyone could learn which payment events were seen.
grant select on table "svetra"."webhook_events" to authenticated;
