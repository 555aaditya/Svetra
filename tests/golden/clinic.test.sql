-- Access tests: run with `supabase test db`. Every check runs as a real user, never the
-- service key, and the whole file rolls back. People: anon, alice, bob, admin, admin without a second factor.
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
-- Call a hole as one person, then undo it. Refused means SQLSTATE 42501; anything else, even an
-- error from the hole's own code, means the call got past who-may-call-it.
create function svetra_test.call(p_role text, p_user uuid, p_sql text, p_aal text default 'aal1') returns text
language plpgsql as $$
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role, 'aal', p_aal)::text, true);
    execute p_sql;
    raise exception 'svetra_undo';
  exception when others then
    return case when sqlstate = '42501' then 'refused' else 'let through' end;
  end;
end $$;
-- What a role may do on a table, as "select, insert, update, delete" or "none".
create function svetra_test.privileges(p_role text, p_table text) returns text language sql as $$
  select coalesce(string_agg(v.p, ', ' order by v.o), 'none')
  from (values (1, 'select'), (2, 'insert'), (3, 'update'), (4, 'delete')) v(o, p)
  where has_table_privilege(p_role, p_table, v.p)
$$;

select plan(45);

-- Test data, inserted as the database owner
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test'), ('5e7a0000-0000-4000-8000-000000000003', 'admin@svetra.test'), ('5e7a0000-0000-4000-8000-000000000004', 'admin.without.a.second.factor@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000003', 'admin', null)$svetra$, 'test data: admin holds admin');
select lives_ok($svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000004', 'admin', null)$svetra$, 'test data: admin without a second factor holds admin');
select lives_ok($svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8000-000000000001', 'svetra a')$svetra$, 'test data: Visit a');
select lives_ok($svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8000-000000000002', 'svetra b')$svetra$, 'test data: Visit b');

-- Checks
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t see Visit a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t see Visit b');
select is(svetra_test.try('anon', null, $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t change Visit a');
select is(svetra_test.try('anon', null, $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t change Visit b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t delete Visit a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t delete Visit b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'anon can''t create Visit c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'anon can''t create Visit d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'alice can see Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t see Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t change Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t change Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t delete Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t delete Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'allowed', 'alice can create Visit c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'alice can''t create Visit d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t see Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'allowed', 'bob can see Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t change Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t change Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t delete Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t delete Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'bob can''t create Visit c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'allowed', 'bob can create Visit d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'allowed', 'admin can see Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'allowed', 'admin can see Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'allowed', 'admin can change Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'allowed', 'admin can change Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'allowed', 'admin can delete Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'allowed', 'admin can delete Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$, 'aal2'), 'denied', 'admin can''t create Visit c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$, 'aal2'), 'denied', 'admin can''t create Visit d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'admin without a second factor can''t see Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'admin without a second factor can''t see Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'admin without a second factor can''t change Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."visit" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'admin without a second factor can''t change Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'admin without a second factor can''t delete Visit a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."visit" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'admin without a second factor can''t delete Visit b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'admin without a second factor can''t create Visit c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."visit" ("id", "patient", "notes") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'admin without a second factor can''t create Visit d');

select * from finish();
rollback;
