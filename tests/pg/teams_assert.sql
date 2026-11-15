-- Behaviour of the compiled teams app: people start teams and add members through holes that may
-- grant only the member role. Every check raises an exception when it fails.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000'), ('b0000000-0000-0000-0000-000000000000'), ('c0000000-0000-0000-0000-000000000000');

-- Alice starts a team: she becomes its member, so she sees it and can add a task.
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ declare t public.team; begin
  t := public.start_team('Flat 1');
  if t.name <> 'Flat 1' then raise exception 'start_team returned the wrong team'; end if;
  perform set_config('svetra_check.team', t.id::text, false);
  if (select count(*) from public.team) <> 1 then raise exception 'alice should see the team she started'; end if;
  insert into public.task (team, author, title) values (t.id, 'a0000000-0000-0000-0000-000000000000', 'Buy milk');
end $$;
-- She can't give herself admin directly: the API can't reach role grants.
do $$ begin
  insert into svetra.role_grants (user_id, role) values ('a0000000-0000-0000-0000-000000000000', 'admin');
  raise exception 'alice gave herself admin';
exception when insufficient_privilege then null; end $$;

-- Bob isn't a member yet: he sees nothing, and can't add himself.
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.team) + (select count(*) from public.task) <> 0 then raise exception 'bob saw a team he isn''t in'; end if;
  perform public.add_member(current_setting('svetra_check.team')::uuid, 'b0000000-0000-0000-0000-000000000000');
  raise exception 'bob added himself to alice''s team';
exception when insufficient_privilege then null; end $$;

-- Alice adds bob; now he sees the team and its task.
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ begin
  perform public.add_member(current_setting('svetra_check.team')::uuid, 'b0000000-0000-0000-0000-000000000000');
  perform public.add_member(current_setting('svetra_check.team')::uuid, 'b0000000-0000-0000-0000-000000000000');
end $$;
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.team) <> 1 then raise exception 'bob should see the team after being added'; end if;
  if (select count(*) from public.task) <> 1 then raise exception 'bob should see the team''s task'; end if;
end $$;

-- Carol, outside the team, sees nothing.
set request.jwt.claim.sub = 'c0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.team) + (select count(*) from public.task) <> 0 then raise exception 'carol saw a team she isn''t in'; end if;
end $$;
reset role;

-- A hole that grants member can't grant admin, even with a hand-edited body.
set role svetra_hole_add_member;
do $$ begin
  insert into svetra.role_grants (user_id, role) values ('c0000000-0000-0000-0000-000000000000', 'admin');
  raise exception 'add_member''s role granted admin';
exception when insufficient_privilege then null; end $$;
reset role;

-- Read back as the owner: exactly alice's and bob's memberships, added once each.
do $$ begin
  if (select count(*) from svetra.role_grants) <> 2 then raise exception 'expected exactly 2 grants, found %', (select count(*) from svetra.role_grants); end if;
  if exists (select 1 from svetra.role_grants where role <> 'member') then raise exception 'a grant other than member was made'; end if;
end $$;
