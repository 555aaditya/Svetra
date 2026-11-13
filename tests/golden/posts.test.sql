-- Access tests: run with `supabase test db`. Every check runs as a real user, never the
-- service key, and the whole file rolls back. People: anon, alice, bob, carol.
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
-- Whether a role may run some function with this name in this schema.
create function svetra_test.executes(p_role text, p_schema text, p_name text) returns boolean language sql as $$
  select coalesce(bool_or(has_function_privilege(p_role, p.oid, 'execute')), false)
  from pg_proc p join pg_namespace n on n.oid = p.pronamespace
  where n.nspname = p_schema and p.proname = p_name
$$;

select plan(61);

-- Test data, inserted as the database owner
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test'), ('5e7a0000-0000-4000-8000-000000000003', 'carol@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "public"."audit_log" ("id", "note") values ('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$, 'test data: AuditLog a');
select lives_ok($svetra$insert into "public"."audit_log" ("id", "note") values ('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$, 'test data: AuditLog b');
select lives_ok($svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000a', '5e7a0000-0000-4000-8000-000000000001', 'svetra a')$svetra$, 'test data: Post a');
select lives_ok($svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000b', '5e7a0000-0000-4000-8000-000000000002', 'svetra b')$svetra$, 'test data: Post b');

-- Checks
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."audit_log" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t see AuditLog a');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."audit_log" ("id", "note") values ('5e7a0000-0000-4000-8001-00010000000c', 'svetra c')$svetra$), 'denied', 'anon can''t create AuditLog c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."audit_log" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t see AuditLog a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."audit_log" ("id", "note") values ('5e7a0000-0000-4000-8001-00010000000c', 'svetra c')$svetra$), 'denied', 'alice can''t create AuditLog c');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'anon can see Post a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'anon can see Post b');
select is(svetra_test.try('anon', null, $svetra$update "public"."post" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t change Post a');
select is(svetra_test.try('anon', null, $svetra$update "public"."post" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t change Post b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t delete Post a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t delete Post b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'anon can''t create Post c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'anon can''t create Post d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can see Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'alice can see Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."post" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can change Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."post" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t change Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can delete Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t delete Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'allowed', 'alice can create Post c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'alice can''t create Post d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."post" set "author" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t set the author of Post a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."post" set "author" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t set the author of Post b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'bob can see Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can see Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."post" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t change Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."post" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can change Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t delete Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can delete Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'bob can''t create Post c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'allowed', 'bob can create Post d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."post" set "author" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t set the author of Post a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."post" set "author" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t set the author of Post b to alice');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'allowed', 'carol can see Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'allowed', 'carol can see Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."post" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."post" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Post a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."post" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Post b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$, 'aal2'), 'denied', 'carol can''t create Post c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."post" ("id", "author", "body") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$, 'aal2'), 'denied', 'carol can''t create Post d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."post" set "author" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t set the author of Post a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."post" set "author" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t set the author of Post b to themselves');
select is(svetra_test.privileges('svetra_hole_write_audit', '"public"."audit_log"'), 'select, insert, update, delete', 'write_audit can change AuditLog');
select is(svetra_test.privileges('svetra_hole_write_audit', '"public"."post"'), 'none', 'write_audit can''t touch Post');
select is(svetra_test.executes('anon', 'public', 'write_audit'), false, 'anon can''t call write_audit');
select is(svetra_test.executes('anon', 'svetra_holes', 'write_audit'), false, 'anon can''t call the inside of write_audit directly');
select is(svetra_test.executes('authenticated', 'public', 'write_audit'), true, 'signed-in people can call write_audit');
select is(svetra_test.executes('authenticated', 'svetra_holes', 'write_audit'), false, 'signed-in people can''t call the inside of write_audit directly');
select is(svetra_test.call('anon', null, $svetra$select "public"."write_audit"()$svetra$), 'refused', 'anon can''t call write_audit on the a rows');
select is(svetra_test.call('anon', null, $svetra$select "public"."write_audit"()$svetra$), 'refused', 'anon can''t call write_audit on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."write_audit"()$svetra$), 'let through', 'alice can call write_audit on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."write_audit"()$svetra$), 'let through', 'alice can call write_audit on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."write_audit"()$svetra$), 'let through', 'bob can call write_audit on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."write_audit"()$svetra$), 'let through', 'bob can call write_audit on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."write_audit"()$svetra$, 'aal2'), 'let through', 'carol can call write_audit on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."write_audit"()$svetra$, 'aal2'), 'let through', 'carol can call write_audit on the b rows');

select * from finish();
rollback;
