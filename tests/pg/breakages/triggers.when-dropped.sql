-- The condition dropped by hand: the hole now runs for every change to an order, not only one to paid.
drop trigger "svetra_notify_paid_update" on "public"."order";
create trigger "svetra_notify_paid_update" after update on "public"."order"
  for each row execute function "svetra_holes"."notify_paid__fire"();
