-- A grant added by hand: the write_audit hole can now read posts, which it never declared.
grant select on table "public"."post" to "svetra_hole_write_audit";
