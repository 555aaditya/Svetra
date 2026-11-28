-- The payment webhook's database function made callable by signed-in people: anyone could post a payment.
grant execute on function "public"."payment_received"(uuid, numeric(12, 2), text) to authenticated;
