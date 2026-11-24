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

select plan(84);

-- Test data, inserted as the database owner
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test'), ('5e7a0000-0000-4000-8000-000000000003', 'carol@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8000-000000000001', 1, 'svetra a')$svetra$, 'test data: Wallet a');
select lives_ok($svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8000-000000000002', 2, 'svetra b')$svetra$, 'test data: Wallet b');

-- Checks
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t see Wallet a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t see Wallet b');
select is(svetra_test.try('anon', null, $svetra$update "public"."wallet" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t change Wallet a');
select is(svetra_test.try('anon', null, $svetra$update "public"."wallet" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t change Wallet b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t delete Wallet a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t delete Wallet b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 3, 'svetra c')$svetra$), 'denied', 'anon can''t create Wallet c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 4, 'svetra d')$svetra$), 'denied', 'anon can''t create Wallet d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'alice can see Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t see Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."wallet" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t change Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."wallet" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t change Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t delete Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t delete Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 3, 'svetra c')$svetra$), 'denied', 'alice can''t create Wallet c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 4, 'svetra d')$svetra$), 'denied', 'alice can''t create Wallet d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."wallet" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t set the owner of Wallet a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."wallet" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t set the owner of Wallet b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t see Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'allowed', 'bob can see Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."wallet" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t change Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."wallet" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t change Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t delete Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t delete Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 3, 'svetra c')$svetra$), 'denied', 'bob can''t create Wallet c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 4, 'svetra d')$svetra$), 'denied', 'bob can''t create Wallet d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."wallet" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t set the owner of Wallet a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."wallet" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t set the owner of Wallet b to alice');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t see Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t see Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."wallet" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."wallet" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Wallet a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."wallet" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Wallet b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 3, 'svetra c')$svetra$, 'aal2'), 'denied', 'carol can''t create Wallet c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."wallet" ("id", "owner", "balance", "label") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 4, 'svetra d')$svetra$, 'aal2'), 'denied', 'carol can''t create Wallet d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."wallet" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Wallet a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."wallet" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Wallet b to themselves');
select is(svetra_test.privileges('svetra_hole_deposit', '"public"."wallet"'), 'select, insert, update, delete', 'deposit can change Wallet');
select is(svetra_test.executes('anon', 'public', 'deposit'), false, 'anon can''t call deposit');
select is(svetra_test.executes('anon', 'svetra_holes', 'deposit'), false, 'anon can''t call the inside of deposit directly');
select is(svetra_test.executes('authenticated', 'public', 'deposit'), true, 'signed-in people can call deposit');
select is(svetra_test.executes('authenticated', 'svetra_holes', 'deposit'), false, 'signed-in people can''t call the inside of deposit directly');
select is(svetra_test.call('anon', null, $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000a', 1)$svetra$), 'refused', 'anon can''t call deposit on the a rows');
select is(svetra_test.call('anon', null, $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000b', 2)$svetra$), 'refused', 'anon can''t call deposit on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000a', 1)$svetra$), 'let through', 'alice can call deposit on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000b', 2)$svetra$), 'refused', 'alice can''t call deposit on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000a', 1)$svetra$), 'refused', 'bob can''t call deposit on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000b', 2)$svetra$), 'let through', 'bob can call deposit on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000a', 1)$svetra$, 'aal2'), 'refused', 'carol can''t call deposit on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000b', 2)$svetra$, 'aal2'), 'refused', 'carol can''t call deposit on the b rows');
select is(svetra_test.call_input('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000a', 0)$svetra$), 'deposit: amount > 0', 'alice calling deposit with amount 0 is refused as bad input: amount > 0');
select is(svetra_test.call_input('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."deposit"('5e7a0000-0000-4000-8001-00010000000a', 10001)$svetra$), 'deposit: amount <= 10000', 'alice calling deposit with amount 10001 is refused as bad input: amount <= 10000');
select is(svetra_test.privileges('svetra_hole_relabel', '"public"."wallet"'), 'select, insert, update, delete', 'relabel can change Wallet');
select is(svetra_test.executes('anon', 'public', 'relabel'), false, 'anon can''t call relabel');
select is(svetra_test.executes('anon', 'svetra_holes', 'relabel'), false, 'anon can''t call the inside of relabel directly');
select is(svetra_test.executes('authenticated', 'public', 'relabel'), true, 'signed-in people can call relabel');
select is(svetra_test.executes('authenticated', 'svetra_holes', 'relabel'), false, 'signed-in people can''t call the inside of relabel directly');
select is(svetra_test.call('anon', null, $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$), 'refused', 'anon can''t call relabel on the a rows');
select is(svetra_test.call('anon', null, $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$), 'refused', 'anon can''t call relabel on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$), 'let through', 'alice can call relabel on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$), 'refused', 'alice can''t call relabel on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$), 'refused', 'bob can''t call relabel on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$), 'let through', 'bob can call relabel on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000a', 'svetra a')$svetra$, 'aal2'), 'refused', 'carol can''t call relabel on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000b', 'svetra b')$svetra$, 'aal2'), 'refused', 'carol can''t call relabel on the b rows');
select is(svetra_test.call_input('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."relabel"('5e7a0000-0000-4000-8001-00010000000a', '100%')$svetra$), 'relabel: label != null and label != ''100%''', 'alice calling relabel with label ''100%'' is refused as bad input: label != null and label != ''100%''');
select is(svetra_test.privileges('svetra_hole_move', '"public"."wallet"'), 'select', 'move can only read Wallet');
select is(svetra_test.executes('anon', 'public', 'move'), false, 'anon can''t call move');
select is(svetra_test.executes('anon', 'svetra_holes', 'move'), false, 'anon can''t call the inside of move directly');
select is(svetra_test.executes('authenticated', 'public', 'move'), true, 'signed-in people can call move');
select is(svetra_test.executes('authenticated', 'svetra_holes', 'move'), false, 'signed-in people can''t call the inside of move directly');
select is(svetra_test.call('anon', null, $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b')$svetra$), 'refused', 'anon can''t call move on the a rows');
select is(svetra_test.call('anon', null, $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8001-00010000000a')$svetra$), 'refused', 'anon can''t call move on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b')$svetra$), 'let through', 'alice can call move on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8001-00010000000a')$svetra$), 'refused', 'alice can''t call move on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b')$svetra$), 'refused', 'bob can''t call move on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8001-00010000000a')$svetra$), 'let through', 'bob can call move on the b rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b')$svetra$, 'aal2'), 'refused', 'carol can''t call move on the a rows');
select is(svetra_test.call('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8001-00010000000a')$svetra$, 'aal2'), 'refused', 'carol can''t call move on the b rows');
select is(svetra_test.call_input('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select "public"."move"('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000a')$svetra$), 'move: source != destination', 'alice calling move with destination the same as source is refused as bad input: source != destination');

select * from finish();
rollback;
