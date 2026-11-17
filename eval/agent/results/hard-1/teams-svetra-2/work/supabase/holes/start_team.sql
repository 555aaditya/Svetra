declare
  v_id uuid;
begin
  if p_caller is null then
    raise exception 'sign in to start a team' using errcode = '42501';
  end if;
  insert into "public"."team" ("name") values (p_team_name) returning "id" into v_id;
  insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values (p_caller, 'member', v_id);
  return v_id;
end;
