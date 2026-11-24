-- The trigger switched off by hand: changing an order no longer starts the hole.
alter table "public"."order" disable trigger "svetra_note_order_change_insert";
