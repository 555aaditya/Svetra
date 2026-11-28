-- Access tests: run with `supabase test db`. Every check runs as a real user, never the
-- service key, and the whole file rolls back. People: anon, alice, bob, carol.
begin;
create extension if not exists pgtap;
-- Run one statement as one person, then undo it. Returns allowed, denied, blocked or the error.
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
    -- A delete that a foreign key refuses: the row was visible and deletable, but another still points at it.
    elsif sqlstate = '23503' and sqlerrm like 'update or delete on table%' then
      return 'blocked';
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
-- Call a hole as one person, then undo it. A refusal for bad input (SQLSTATE 22023) returns its
-- message; refused (42501) and anything else return what `call` does.
create function svetra_test.call_input(p_role text, p_user uuid, p_sql text, p_aal text default 'aal1') returns text
language plpgsql as $$
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role, 'aal', p_aal)::text, true);
    execute p_sql;
    raise exception 'svetra_undo';
  exception when others then
    return case when sqlstate = '42501' then 'refused' when sqlstate = '22023' then sqlerrm else 'let through' end;
  end;
end $$;

select plan(110);

-- Test data, inserted as the database owner
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test'), ('5e7a0000-0000-4000-8000-000000000003', 'carol@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8000-000000000001', 1, 'open')$svetra$, 'test data: Invoice a');
select lives_ok($svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8000-000000000002', 2, 'paid')$svetra$, 'test data: Invoice b');
select lives_ok($svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000a', '5e7a0000-0000-4000-8001-00010000000a', 1)$svetra$, 'test data: Payment a');
select lives_ok($svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000b', '5e7a0000-0000-4000-8001-00010000000b', 2)$svetra$, 'test data: Payment b');

-- Checks
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t see Invoice a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t see Invoice b');
select is(svetra_test.try('anon', null, $svetra$update "public"."invoice" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t change Invoice a');
select is(svetra_test.try('anon', null, $svetra$update "public"."invoice" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t change Invoice b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t delete Invoice a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t delete Invoice b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 3, 'open')$svetra$), 'denied', 'anon can''t create Invoice c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 4, 'paid')$svetra$), 'denied', 'anon can''t create Invoice d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'alice can see Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t see Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."invoice" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t change Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."invoice" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t change Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t delete Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t delete Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 3, 'open')$svetra$), 'denied', 'alice can''t create Invoice c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 4, 'paid')$svetra$), 'denied', 'alice can''t create Invoice d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."invoice" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t set the owner of Invoice a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."invoice" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t set the owner of Invoice b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t see Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'allowed', 'bob can see Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."invoice" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t change Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."invoice" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t change Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t delete Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t delete Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 3, 'open')$svetra$), 'denied', 'bob can''t create Invoice c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 4, 'paid')$svetra$), 'denied', 'bob can''t create Invoice d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."invoice" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t set the owner of Invoice a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."invoice" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t set the owner of Invoice b to alice');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t see Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t see Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."invoice" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."invoice" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."invoice" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 3, 'open')$svetra$, 'aal2'), 'denied', 'carol can''t create Invoice c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."invoice" ("id", "owner", "amount", "status") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 4, 'paid')$svetra$, 'aal2'), 'denied', 'carol can''t create Invoice d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."invoice" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Invoice a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."invoice" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Invoice b to themselves');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t see Payment a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t see Payment b');
select is(svetra_test.try('anon', null, $svetra$update "public"."payment" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t change Payment a');
select is(svetra_test.try('anon', null, $svetra$update "public"."payment" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t change Payment b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t delete Payment a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t delete Payment b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8001-00010000000a', 3)$svetra$), 'denied', 'anon can''t create Payment c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8001-00010000000b', 4)$svetra$), 'denied', 'anon can''t create Payment d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can see Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t see Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."payment" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t change Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."payment" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t change Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t delete Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t delete Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8001-00010000000a', 3)$svetra$), 'denied', 'alice can''t create Payment c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8001-00010000000b', 4)$svetra$), 'denied', 'alice can''t create Payment d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."payment" set "invoice" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t point the invoice of Payment a at Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."payment" set "invoice" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t point the invoice of Payment b at Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t see Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can see Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."payment" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t change Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."payment" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t change Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t delete Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t delete Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8001-00010000000a', 3)$svetra$), 'denied', 'bob can''t create Payment c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8001-00010000000b', 4)$svetra$), 'denied', 'bob can''t create Payment d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."payment" set "invoice" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t point the invoice of Payment a at Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."payment" set "invoice" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t point the invoice of Payment b at Invoice a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t see Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t see Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."payment" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."payment" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Payment a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."payment" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Payment b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8001-00010000000a', 3)$svetra$, 'aal2'), 'denied', 'carol can''t create Payment c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."payment" ("id", "invoice", "amount") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8001-00010000000b', 4)$svetra$, 'aal2'), 'denied', 'carol can''t create Payment d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."payment" set "invoice" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t point the invoice of Payment a at Invoice b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."payment" set "invoice" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t point the invoice of Payment b at Invoice a');
select is(svetra_test.privileges('svetra_hole_payment_received', '"public"."invoice"'), 'select, insert, update, delete', 'payment_received can change Invoice');
select is(svetra_test.privileges('svetra_hole_payment_received', '"public"."payment"'), 'select, insert, update, delete', 'payment_received can change Payment');
select is(svetra_test.executes('anon', 'public', 'payment_received'), false, 'anon can''t call payment_received');
select is(svetra_test.executes('anon', 'svetra_holes', 'payment_received'), false, 'anon can''t call the inside of payment_received directly');
select is(svetra_test.executes('authenticated', 'public', 'payment_received'), true, 'signed-in people can call payment_received');
select is(svetra_test.executes('authenticated', 'svetra_holes', 'payment_received'), false, 'signed-in people can''t call the inside of payment_received directly');
select is(svetra_test.call('anon', null, $svetra$select "public"."payment_received"('5e7a0000-0000-4000-8001-00010000000a', 1, 'svetra a')$svetra$), 'refused', 'anon can''t call payment_received on the a rows');
select is(svetra_test.call('anon', null, $svetra$select "public"."payment_received"('5e7a0000-0000-4000-8001-00010000000b', 2, 'svetra b')$svetra$), 'refused', 'anon can''t call payment_received on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."payment_received"('5e7a0000-0000-4000-8001-00010000000a', 1, 'svetra a')$svetra$), 'let through', 'alice can call payment_received on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."payment_received"('5e7a0000-0000-4000-8001-00010000000b', 2, 'svetra b')$svetra$), 'let through', 'alice can call payment_received on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."payment_received"('5e7a0000-0000-4000-8001-00010000000a', 1, 'svetra a')$svetra$), 'let through', 'bob can call payment_received on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."payment_received"('5e7a0000-0000-4000-8001-00010000000b', 2, 'svetra b')$svetra$), 'let through', 'bob can call payment_received on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."payment_received"('5e7a0000-0000-4000-8001-00010000000a', 1, 'svetra a')$svetra$, 'aal2'), 'let through', 'carol can call payment_received on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."payment_received"('5e7a0000-0000-4000-8001-00010000000b', 2, 'svetra b')$svetra$, 'aal2'), 'let through', 'carol can call payment_received on the b rows');
select is(svetra_test.privileges('svetra_hole_form_submitted', '"public"."invoice"'), 'select', 'form_submitted can only read Invoice');
select is(svetra_test.privileges('svetra_hole_form_submitted', '"public"."payment"'), 'none', 'form_submitted can''t touch Payment');
select is(svetra_test.executes('anon', 'public', 'form_submitted'), false, 'anon can''t call form_submitted');
select is(svetra_test.executes('anon', 'svetra_holes', 'form_submitted'), false, 'anon can''t call the inside of form_submitted directly');
select is(svetra_test.executes('authenticated', 'public', 'form_submitted'), true, 'signed-in people can call form_submitted');
select is(svetra_test.executes('authenticated', 'svetra_holes', 'form_submitted'), false, 'signed-in people can''t call the inside of form_submitted directly');
select is(svetra_test.call('anon', null, $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$), 'refused', 'anon can''t call form_submitted on the a rows');
select is(svetra_test.call('anon', null, $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$), 'refused', 'anon can''t call form_submitted on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$), 'let through', 'alice can call form_submitted on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$), 'let through', 'alice can call form_submitted on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$), 'let through', 'bob can call form_submitted on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$), 'let through', 'bob can call form_submitted on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$, 'aal2'), 'let through', 'carol can call form_submitted on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$, 'aal2'), 'let through', 'carol can call form_submitted on the b rows');
select is(svetra_test.call_input('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."form_submitted"('5e7a0000-0000-4000-8001-00010000000a', '')$svetra$), 'form_submitted: note != ''''', 'alice calling form_submitted with note '''' is refused as bad input: note != ''''');

select * from finish();
rollback;
