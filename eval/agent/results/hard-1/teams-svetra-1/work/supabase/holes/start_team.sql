declare
  v_team uuid;
begin
  insert into "public"."team" ("name") values (p_team_name) returning "id" into v_team;
  insert into "svetra"."role_grants" ("user_id", "role", "scope_id")
    values (p_caller, 'member', v_team);
  return v_team;
end;
