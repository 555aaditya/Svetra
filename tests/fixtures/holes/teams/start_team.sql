  declare
    v_team "public"."team";
  begin
    insert into "public"."team" ("name") values (p_name) returning * into v_team;
    insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values (p_caller, 'member', v_team."id");
    return v_team;
  end;
