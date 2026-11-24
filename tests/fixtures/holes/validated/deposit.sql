  update "public"."wallet" set "balance" = "balance" + p_amount where "id" = p_wallet;
