-- The trigger function emptied by hand: the trigger is there and enabled, but nothing runs.
create or replace function "svetra_holes"."note_order_change__fire"()
  returns trigger language plpgsql security definer set search_path = '' as $$ begin return null; end $$;
