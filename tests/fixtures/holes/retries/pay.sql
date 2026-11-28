  declare
    v_pay "public"."payment";
  begin
    update "public"."wallet" set "balance" = "balance" + p_amount where "id" = p_wallet;
    insert into "public"."payment" ("id", "wallet", "amount") values (gen_random_uuid(), p_wallet, p_amount)
      returning * into v_pay;
    return v_pay;
  end;
