-- The guard trigger is gone: a reviewer can approve and retitle in one update.
drop trigger "svetra_guard" on "public"."doc";
