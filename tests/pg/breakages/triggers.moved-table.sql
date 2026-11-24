-- The trigger moved to another table: orders no longer start the hole, and notifications do.
drop trigger "svetra_note_order_change_delete" on "public"."order";
create trigger "svetra_note_order_change_delete" after delete on "public"."notification"
  for each row when (old."message" is not null) execute function "svetra_holes"."note_order_change__fire"();
