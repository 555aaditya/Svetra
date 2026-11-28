-- The inside of the payment webhook made callable by the service role: a caller could skip the guard and the repeat check.
grant execute on function "svetra_holes"."payment_received"(uuid, uuid, numeric(12, 2), text) to service_role;
