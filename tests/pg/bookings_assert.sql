-- Behaviour of the compiled bookings example: a global role (admin), a role scoped to one
-- location (staff), and customers who own their bookings. After every write, read back.
insert into auth.users values
  ('c1000000-0000-0000-0000-000000000000'), ('c2000000-0000-0000-0000-000000000000'),
  ('5a000000-0000-0000-0000-000000000000'), ('5b000000-0000-0000-0000-000000000000'),
  ('ad000000-0000-0000-0000-000000000000'), ('00000000-0000-0000-0000-000000000000');
insert into public.location (id, name) values ('10000000-0000-0000-0000-000000000000', 'North'), ('20000000-0000-0000-0000-000000000000', 'South');
insert into public.booking (id, customer, location, starts) values
  ('b1000000-0000-0000-0000-000000000000', 'c1000000-0000-0000-0000-000000000000', '10000000-0000-0000-0000-000000000000', '2026-12-01 09:00+00'),
  ('b2000000-0000-0000-0000-000000000000', 'c2000000-0000-0000-0000-000000000000', '20000000-0000-0000-0000-000000000000', '2026-12-01 10:00+00');
-- Grants are made by the table owner (the service role, in Supabase): staff at North, staff at South, one admin.
insert into svetra.role_grants (user_id, role, scope_id) values
  ('5a000000-0000-0000-0000-000000000000', 'staff', '10000000-0000-0000-0000-000000000000'),
  ('5b000000-0000-0000-0000-000000000000', 'staff', '20000000-0000-0000-0000-000000000000'),
  ('ad000000-0000-0000-0000-000000000000', 'admin', null);

-- Logged-out visitors see nothing.
set role anon;
do $$ begin
  if (select count(*) from public.booking) + (select count(*) from public.location) <> 0 then raise exception 'anon should see nothing'; end if;
end $$;
reset role;

set role authenticated;
-- A signed-in person with no role and no bookings sees nothing, and can't grant themselves a role.
set request.jwt.claim.sub = '00000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.booking) + (select count(*) from public.location) <> 0 then raise exception 'a user with no role should see nothing'; end if;
  if (select svetra.has_role('admin')) then raise exception 'has_role says a user with no grant is admin'; end if;
  begin insert into svetra.role_grants values ('00000000-0000-0000-0000-000000000000', 'admin', null); raise exception 'a user granted themselves admin';
  exception when insufficient_privilege then null; end;
  begin perform count(*) from svetra.role_grants; raise exception 'a user read the role grants';
  exception when insufficient_privilege then null; end;
end $$;

-- Customer 1 sees only their booking, books at South, and can't book as someone else.
set request.jwt.claim.sub = 'c1000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.booking) <> 1 then raise exception 'customer 1 should see only their booking'; end if;
  begin insert into public.booking (customer, location, starts) values ('c2000000-0000-0000-0000-000000000000', '10000000-0000-0000-0000-000000000000', now()); raise exception 'customer 1 booked as customer 2';
  exception when insufficient_privilege then null; end;
end $$;
insert into public.booking (id, customer, location, starts) values ('b3000000-0000-0000-0000-000000000000', 'c1000000-0000-0000-0000-000000000000', '20000000-0000-0000-0000-000000000000', '2026-12-02 09:00+00');
update public.booking set starts = '2030-01-01 00:00+00';   -- customers have no update rule: changes nothing
do $$ begin
  if (select count(*) from public.booking) <> 2 then raise exception 'customer 1 should read back their new booking'; end if;
  if exists (select 1 from public.booking where starts = '2030-01-01 00:00+00') then raise exception 'a customer changed a booking'; end if;
end $$;

-- Customer 2 still sees only their own.
set request.jwt.claim.sub = 'c2000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.booking) <> 1 then raise exception 'customer 2 should see only their own booking'; end if;
end $$;

-- Staff at North see North's bookings and location only; they can change North's bookings, not South's, and delete none.
set request.jwt.claim.sub = '5a000000-0000-0000-0000-000000000000';
update public.booking set starts = '2026-12-01 11:00+00' where id = 'b1000000-0000-0000-0000-000000000000';
update public.booking set starts = '2030-01-01 00:00+00' where id = 'b2000000-0000-0000-0000-000000000000';
delete from public.booking where id = 'b1000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.booking) <> 1 then raise exception 'north staff should see only north bookings'; end if;
  if (select string_agg(name, ',') from public.location) is distinct from 'North' then raise exception 'north staff should see only the north location'; end if;
  if (select starts from public.booking where id = 'b1000000-0000-0000-0000-000000000000') <> '2026-12-01 11:00+00' then raise exception 'north staff could not change a north booking'; end if;
end $$;
set request.jwt.claim.sub = 'c1000000-0000-0000-0000-000000000000';
do $$ begin
  if (select starts from public.booking where id = 'b1000000-0000-0000-0000-000000000000') <> '2026-12-01 11:00+00' then raise exception 'customer 1 should see the staff change'; end if;
end $$;

-- Staff at South see both South bookings, including customer 1's new one.
set request.jwt.claim.sub = '5b000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.booking) <> 2 then raise exception 'south staff should see both south bookings'; end if;
end $$;

-- The admin sees everything and can delete.
set request.jwt.claim.sub = 'ad000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.booking) <> 3 then raise exception 'the admin should see all bookings'; end if;
  if (select count(*) from public.location) <> 2 then raise exception 'the admin should see all locations'; end if;
end $$;
delete from public.booking where id = 'b2000000-0000-0000-0000-000000000000';
set request.jwt.claim.sub = 'c2000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.booking) <> 0 then raise exception 'customer 2''s booking should be gone after the admin deleted it'; end if;
end $$;
reset role;

-- As the table owner, bypassing row-level security: exactly the allowed writes happened.
do $$ begin
  if (select count(*) from public.booking) <> 2 then raise exception 'expected 2 bookings in total'; end if;
  if (select starts from public.booking where id = 'b1000000-0000-0000-0000-000000000000') <> '2026-12-01 11:00+00' then raise exception 'b1 should have the staff change'; end if;
  if exists (select 1 from public.booking where starts = '2030-01-01 00:00+00') then raise exception 'a blocked update took effect'; end if;
  if (select count(*) from svetra.role_grants) <> 3 then raise exception 'role grants changed'; end if;
end $$;
