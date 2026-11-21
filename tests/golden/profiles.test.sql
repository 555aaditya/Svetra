-- Access tests: run with `supabase test db`. Every check runs as a real user, never the
-- service key, and the whole file rolls back. People: anon, alice, bob, carol, admin.
begin;
create extension if not exists pgtap;
-- Run one statement as one person, then undo it. Returns allowed, denied or the error.
-- p_before, if given, is run first as the database owner (to give a person an extra role).
create schema svetra_test;
create function svetra_test.try(p_role text, p_user uuid, p_sql text, p_aal text default 'aal1', p_before text default null) returns text
language plpgsql as $$
declare n bigint;
begin
  begin
    -- p_before runs as the database owner, then is undone with the statement.
    if p_before is not null then execute p_before; end if;
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
-- Whether a role may run some function with this name in this schema.
create function svetra_test.executes(p_role text, p_schema text, p_name text) returns boolean language sql as $$
  select coalesce(bool_or(has_function_privilege(p_role, p.oid, 'execute')), false)
  from pg_proc p join pg_namespace n on n.oid = p.pronamespace
  where n.nspname = p_schema and p.proname = p_name
$$;

select plan(113);

-- Test data, inserted as the database owner
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test'), ('5e7a0000-0000-4000-8000-000000000003', 'carol@svetra.test'), ('5e7a0000-0000-4000-8000-000000000004', 'admin@svetra.test'), ('5e7a0000-0000-4000-8000-000000000fff', 'fourth@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000004', 'admin', null)$svetra$, 'test data: admin holds admin');
select lives_ok($svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000001', 'svetra a', 'ONLINE')$svetra$, 'test data: Profile a');
select lives_ok($svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000002', 'svetra b', 'OFFLINE')$svetra$, 'test data: Profile b');
select lives_ok($svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000a', '5e7a0000-0000-4000-8000-000000000001', 'svetra a')$svetra$, 'test data: Note a');
select lives_ok($svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000b', '5e7a0000-0000-4000-8000-000000000002', 'svetra b')$svetra$, 'test data: Note b');

-- Checks
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice can''t give herself the admin role');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'denied', 'anon can''t see Profile a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'denied', 'anon can''t see Profile b');
select is(svetra_test.try('anon', null, $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'denied', 'anon can''t change Profile a');
select is(svetra_test.try('anon', null, $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'denied', 'anon can''t change Profile b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'denied', 'anon can''t delete Profile a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'denied', 'anon can''t delete Profile b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000003', 'svetra c', 'ONLINE')$svetra$), 'denied', 'anon can''t create Profile c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000fff', 'svetra d', 'OFFLINE')$svetra$), 'denied', 'anon can''t create Profile d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'allowed', 'alice can see Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'allowed', 'alice can see Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'allowed', 'alice can change Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'denied', 'alice can''t change Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'allowed', 'alice can delete Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'denied', 'alice can''t delete Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000003', 'svetra c', 'ONLINE')$svetra$), 'denied', 'alice can''t create Profile c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000fff', 'svetra d', 'OFFLINE')$svetra$), 'denied', 'alice can''t create Profile d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'allowed', 'bob can see Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'allowed', 'bob can see Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'denied', 'bob can''t change Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'allowed', 'bob can change Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'denied', 'bob can''t delete Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'allowed', 'bob can delete Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000003', 'svetra c', 'ONLINE')$svetra$), 'denied', 'bob can''t create Profile c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000fff', 'svetra d', 'OFFLINE')$svetra$), 'denied', 'bob can''t create Profile d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$, 'aal2'), 'allowed', 'carol can see Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$, 'aal2'), 'allowed', 'carol can see Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$, 'aal2'), 'denied', 'carol can''t change Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$, 'aal2'), 'denied', 'carol can''t change Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$, 'aal2'), 'denied', 'carol can''t delete Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$, 'aal2'), 'denied', 'carol can''t delete Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000003', 'svetra c', 'ONLINE')$svetra$, 'aal2'), 'allowed', 'carol can create Profile c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000fff', 'svetra d', 'OFFLINE')$svetra$, 'aal2'), 'denied', 'carol can''t create Profile d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'allowed', 'admin can see Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'allowed', 'admin can see Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'denied', 'admin can''t change Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'denied', 'admin can''t change Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$), 'allowed', 'admin can delete Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$), 'allowed', 'admin can delete Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000003', 'svetra c', 'ONLINE')$svetra$), 'denied', 'admin can''t create Profile c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000fff', 'svetra d', 'OFFLINE')$svetra$), 'denied', 'admin can''t create Profile d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can see Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can see Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can change Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."profile" set "id" = "id" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t change Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000001'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can delete Profile a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."profile" where "id" = '5e7a0000-0000-4000-8000-000000000002'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can delete Profile b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000003', 'svetra c', 'ONLINE')$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t create Profile c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."profile" ("id", "username", "status") values ('5e7a0000-0000-4000-8000-000000000fff', 'svetra d', 'OFFLINE')$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t create Profile d');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t see Note a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t see Note b');
select is(svetra_test.try('anon', null, $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t change Note a');
select is(svetra_test.try('anon', null, $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t change Note b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t delete Note a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t delete Note b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'anon can''t create Note c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'anon can''t create Note d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can see Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t see Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can change Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t change Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can delete Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t delete Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'allowed', 'alice can create Note c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'alice can''t create Note d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t set the author of Note a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t set the author of Note b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t see Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can see Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t change Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can change Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t delete Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can delete Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'bob can''t create Note c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'allowed', 'bob can create Note d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t set the author of Note a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t set the author of Note b to alice');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t see Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t see Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$, 'aal2'), 'denied', 'carol can''t create Note c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$, 'aal2'), 'denied', 'carol can''t create Note d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t set the author of Note a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t set the author of Note b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'admin can''t see Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'admin can''t see Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'admin can''t change Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'admin can''t change Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'admin can''t delete Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'admin can''t delete Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'admin can''t create Note c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'admin can''t create Note d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000004' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'admin can''t set the author of Note a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000004', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000004' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'admin can''t set the author of Note b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can see Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t see Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can change Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."note" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t change Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can delete Note a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."note" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t delete Note b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'allowed', 'alice who is also admin can create Note c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."note" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t create Note d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t set the author of Note a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."note" set "author" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal1', $svetra$insert into "svetra"."role_grants" ("user_id", "role", "scope_id") values ('5e7a0000-0000-4000-8000-000000000001', 'admin', null)$svetra$), 'denied', 'alice who is also admin can''t set the author of Note b to themselves');

select * from finish();
rollback;
