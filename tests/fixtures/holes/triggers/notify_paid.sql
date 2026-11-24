  -- p_old is the order before the change and p_new the order after it.
  insert into "public"."notification" ("owner", "message")
    select p_new."owner", 'Order for ' || c."name" || ' is paid'
    from "public"."customer" c where c."id" = p_new."customer";
