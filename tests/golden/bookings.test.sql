-- Access tests: run with `supabase test db`. Every check runs as a real user, never the
-- service key, and the whole file rolls back. People: anon, alice, bob, admin, staff at Location a.
begin;
create extension if not exists pgtap;
-- Run one statement as one person, then undo it. Returns allowed, denied or the error.
create schema svetra_test;
create function svetra_test.try(p_role text, p_user uuid, p_sql text, p_aal text default 'aal1') returns text
language plpgsql as $$
declare n bigint;
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role, 'aal', p_aal)::text, true);
    execute p_sql;
    get diagnostics n = row_count;
    raise exception 'svetra_undo';
  exception when others then
    if sqlerrm = 'svetra_undo' then
      return case when n > 0 then 'allowed' else 'denied' end;
    elsif sqlstate = '42501' then
      return 'denied';
    end if;
    return 'error ' || sqlstate || ': ' || sqlerrm;
  end;
end $$;

select plan(87);

-- Test data, inserted as the database owner
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test'), ('5e7a0000-0000-4000-8000-000000000003', 'admin@svetra.test'), ('5e7a0000-0000-4000-8000-000000000004', 'staff.at.Location.a@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000003', 'admin', null)$svetra$, 'test data: admin holds admin');
select lives_ok($svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000004', 'staff', '5e7a0000-0000-4000-8001-00010000000a')$svetra$, 'test data: staff at Location a holds staff');
select lives_ok($svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$, 'test data: Location a');
select lives_ok($svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$, 'test data: Location b');
select lives_ok($svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000a', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', '2030-01-01T10:00:00Z')$svetra$, 'test data: Booking a');
select lives_ok($svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000b', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', '2030-01-02T10:00:00Z')$svetra$, 'test data: Booking b');

-- Checks
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t see Location a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t see Location b');
select is(svetra_test.try('anon', null, $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t change Location a');
select is(svetra_test.try('anon', null, $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t change Location b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t delete Location a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t delete Location b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000c', 'svetra c')$svetra$), 'denied', 'anon can''t create Location c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000d', 'svetra d')$svetra$), 'denied', 'anon can''t create Location d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t see Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t see Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t change Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t change Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t delete Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t delete Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000c', 'svetra c')$svetra$), 'denied', 'alice can''t create Location c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000d', 'svetra d')$svetra$), 'denied', 'alice can''t create Location d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t see Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t see Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t change Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t change Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t delete Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t delete Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000c', 'svetra c')$svetra$), 'denied', 'bob can''t create Location c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000d', 'svetra d')$svetra$), 'denied', 'bob can''t create Location d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'admin can see Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'allowed', 'admin can see Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'admin can change Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'allowed', 'admin can change Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'admin can''t delete Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'admin can''t delete Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000c', 'svetra c')$svetra$), 'allowed', 'admin can create Location c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000d', 'svetra d')$svetra$), 'allowed', 'admin can create Location d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'staff at Location a can see Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'staff at Location a can''t see Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'staff at Location a can''t change Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."location" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'staff at Location a can''t change Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'staff at Location a can''t delete Location a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."location" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'staff at Location a can''t delete Location b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000c', 'svetra c')$svetra$), 'denied', 'staff at Location a can''t create Location c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."location" ("id", "name") values ('5e7a0000-0000-4000-8001-00010000000d', 'svetra d')$svetra$), 'denied', 'staff at Location a can''t create Location d');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t see Booking a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t see Booking b');
select is(svetra_test.try('anon', null, $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t change Booking a');
select is(svetra_test.try('anon', null, $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t change Booking b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t delete Booking a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t delete Booking b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'anon can''t create Booking c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'anon can''t create Booking d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can see Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t see Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t change Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t change Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t delete Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t delete Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', '2030-01-03T10:00:00Z')$svetra$), 'allowed', 'alice can create Booking c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'alice can''t create Booking d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t see Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can see Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t change Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t change Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t delete Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t delete Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'bob can''t create Booking c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', '2030-01-04T10:00:00Z')$svetra$), 'allowed', 'bob can create Booking d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'admin can see Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'admin can see Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'admin can change Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'admin can change Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'admin can delete Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'admin can delete Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'admin can''t create Booking c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'admin can''t create Booking d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'staff at Location a can see Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'staff at Location a can''t see Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'staff at Location a can change Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."booking" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'staff at Location a can''t change Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'staff at Location a can''t delete Booking a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."booking" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'staff at Location a can''t delete Booking b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'staff at Location a can''t create Booking c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."booking" ("id", "customer", "location", "starts") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'staff at Location a can''t create Booking d');

select * from finish();
rollback;
