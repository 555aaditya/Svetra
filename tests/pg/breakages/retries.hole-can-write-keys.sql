-- The pay hole's role allowed to write the key table: its body could forge or erase a key.
grant insert, update, delete on table "svetra"."hole_keys" to "svetra_hole_pay";
