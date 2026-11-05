-- Clinic: the admin role needs a second factor. Each statement runs in autocommit, so every
-- readback sees committed data.
insert into auth.users (id) values
  ('a0000000-0000-0000-0000-000000000001'),
  ('b0000000-0000-0000-0000-000000000002'),
  ('ad000000-0000-0000-0000-000000000003');
insert into svetra.role_grants values ('ad000000-0000-0000-0000-000000000003', 'admin', null);
insert into public.visit values
  ('11111111-0000-0000-0000-000000000001', 'a0000000-0000-0000-0000-000000000001', 'alice'),
  ('22222222-0000-0000-0000-000000000002', 'b0000000-0000-0000-0000-000000000002', 'bob');

set role authenticated;

-- The admin signed in with a password only: just another signed-in person.
set request.jwt.claim.sub = 'ad000000-0000-0000-0000-000000000003';
set request.jwt.claims = '{"aal": "aal1"}';
do $$ begin
  if (select count(*) from public.visit) <> 0 then raise exception 'admin without a second factor should see no visits'; end if;
  if (select svetra.has_role('admin')) then raise exception 'has_role should need aal2 for admin'; end if;
end $$;
delete from public.visit;
reset role;
do $$ begin
  if (select count(*) from public.visit) <> 2 then raise exception 'admin without a second factor deleted visits'; end if;
end $$;

-- The same admin after verifying an authenticator app.
set role authenticated;
set request.jwt.claim.sub = 'ad000000-0000-0000-0000-000000000003';
set request.jwt.claims = '{"aal": "aal2"}';
do $$ begin
  if (select count(*) from public.visit) <> 2 then raise exception 'admin with a second factor should see both visits'; end if;
end $$;

-- A patient is unaffected by the setting.
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000001';
set request.jwt.claims = '{"aal": "aal1"}';
do $$ begin
  if (select count(*) from public.visit) <> 1 then raise exception 'alice should see only her visit'; end if;
end $$;
reset role;
