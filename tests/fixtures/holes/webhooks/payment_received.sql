  insert into "public"."payment" ("id", "invoice", "amount") values (gen_random_uuid(), p_invoice, p_paid);
  update "public"."invoice" set "status" = 'paid' where "id" = p_invoice;
