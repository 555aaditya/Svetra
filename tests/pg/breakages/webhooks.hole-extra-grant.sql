-- The form webhook's role given a right it didn't declare: its body could change invoices.
grant update on table "public"."invoice" to "svetra_hole_form_submitted";
