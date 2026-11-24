  -- p_new is null on delete and p_old is null on insert.
  insert into "public"."notification" ("owner", "message")
    values (
      coalesce(p_new."owner", p_old."owner"),
      case when p_new is null then 'An order was deleted' else 'An order was placed' end
    );
