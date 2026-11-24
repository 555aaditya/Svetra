-- A grant added by hand: the notify_paid hole can now create customers, which it never declared.
grant insert on table "public"."customer" to "svetra_hole_notify_paid";
