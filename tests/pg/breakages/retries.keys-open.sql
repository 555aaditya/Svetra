-- The retry key table readable by signed-in people: anyone could learn whether another person's key exists.
grant select on table "svetra"."hole_keys" to authenticated;
