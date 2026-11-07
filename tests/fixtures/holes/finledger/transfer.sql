  declare
    v_tx "public"."transaction";
  begin
    update "public"."account" set "balance" = "balance" - p_amount where "id" = p_source;
    update "public"."account" set "balance" = "balance" + p_amount where "id" = p_destination;
    insert into "public"."transaction" ("source", "destination", "amount", "status")
      values (p_source, p_destination, p_amount, 'committed')
      returning * into v_tx;
    return v_tx;
  end;
