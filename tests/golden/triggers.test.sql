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
create function svetra_test.noop() returns trigger language plpgsql as $$ begin return null; end $$;
create function svetra_test.when_of(p_trigger oid) returns text language sql as $$
  select substring(pg_get_triggerdef(p_trigger) from ' WHEN \((.*)\) EXECUTE ') $$;
create function svetra_test.trigger_when(p_table text, p_event text, p_cond text) returns text
language plpgsql as $$
declare r text;
begin
  if p_cond is null then return null; end if;
  execute format('create trigger svetra_test_when after %s on %s for each row when (%s) execute function svetra_test.noop()', p_event, p_table, p_cond);
  select svetra_test.when_of(t.oid) into r from pg_trigger t where t.tgname = 'svetra_test_when';
  execute format('drop trigger svetra_test_when on %s', p_table);
  return r;
end $$;

select plan(164);

-- Triggered holes: what is declared, checked before the test data goes in
select is((select count(*) from pg_roles r, pg_proc p join pg_namespace n on n.oid = p.pronamespace where r.rolname in ('anon', 'authenticated', 'service_role') and n.nspname = 'svetra_holes' and p.proname in ('notify_paid', 'notify_paid__fire') and has_function_privilege(r.rolname, p.oid, 'execute')), 0::bigint, 'no one calling the API can run notify_paid, or the function that fires it');
select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'public' and p.proname = 'notify_paid'), 0::bigint, 'notify_paid has no function the app can call');
select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'svetra_holes' and p.proname = 'notify_paid' and pg_get_userbyid(p.proowner) = 'svetra_hole_notify_paid' and p.prosecdef and p.proconfig = array['search_path=""']), 1::bigint, 'notify_paid runs with its own role and an empty search path');
select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'svetra_holes' and p.proname = 'notify_paid__fire' and p.prosecdef and p.proconfig = array['search_path=""']), 1::bigint, 'notify_paid__fire runs with the owner''s rights and an empty search path');
select is((select count(*) from pg_trigger t where t.tgrelid = '"public"."order"'::regclass and t.tgname = 'svetra_notify_paid_update' and t.tgenabled = 'O' and t.tgtype = 17 and t.tgfoid = '"svetra_holes"."notify_paid__fire"()'::regprocedure), 1::bigint, 'notify_paid runs after a row of Order is updated, once per row');
select is((select svetra_test.when_of(t.oid) from pg_trigger t where t.tgrelid = '"public"."order"'::regclass and t.tgname = 'svetra_notify_paid_update'), svetra_test.trigger_when('"public"."order"', 'update', 'new."status" = ''paid'' and old."status" <> ''paid'''), 'notify_paid fires on update only when status == ''paid'' and old.status != ''paid''');
select is((select count(*) from pg_trigger t where t.tgfoid = '"svetra_holes"."notify_paid__fire"()'::regprocedure and not t.tgisinternal), 1::bigint, 'nothing else fires notify_paid');
select is((select count(*) from pg_roles r, pg_proc p join pg_namespace n on n.oid = p.pronamespace where r.rolname in ('anon', 'authenticated', 'service_role') and n.nspname = 'svetra_holes' and p.proname in ('note_order_change', 'note_order_change__fire') and has_function_privilege(r.rolname, p.oid, 'execute')), 0::bigint, 'no one calling the API can run note_order_change, or the function that fires it');
select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'public' and p.proname = 'note_order_change'), 0::bigint, 'note_order_change has no function the app can call');
select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'svetra_holes' and p.proname = 'note_order_change' and pg_get_userbyid(p.proowner) = 'svetra_hole_note_order_change' and p.prosecdef and p.proconfig = array['search_path=""']), 1::bigint, 'note_order_change runs with its own role and an empty search path');
select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'svetra_holes' and p.proname = 'note_order_change__fire' and p.prosecdef and p.proconfig = array['search_path=""']), 1::bigint, 'note_order_change__fire runs with the owner''s rights and an empty search path');
select is((select count(*) from pg_trigger t where t.tgrelid = '"public"."order"'::regclass and t.tgname = 'svetra_note_order_change_insert' and t.tgenabled = 'O' and t.tgtype = 5 and t.tgfoid = '"svetra_holes"."note_order_change__fire"()'::regprocedure), 1::bigint, 'note_order_change runs after a row of Order is inserted, once per row');
select is((select svetra_test.when_of(t.oid) from pg_trigger t where t.tgrelid = '"public"."order"'::regclass and t.tgname = 'svetra_note_order_change_insert'), svetra_test.trigger_when('"public"."order"', 'insert', 'new."total" > 0'), 'note_order_change fires on insert only when total > 0');
select is((select count(*) from pg_trigger t where t.tgrelid = '"public"."order"'::regclass and t.tgname = 'svetra_note_order_change_delete' and t.tgenabled = 'O' and t.tgtype = 9 and t.tgfoid = '"svetra_holes"."note_order_change__fire"()'::regprocedure), 1::bigint, 'note_order_change runs after a row of Order is deleted, once per row');
select is((select svetra_test.when_of(t.oid) from pg_trigger t where t.tgrelid = '"public"."order"'::regclass and t.tgname = 'svetra_note_order_change_delete'), svetra_test.trigger_when('"public"."order"', 'delete', 'old."total" > 0'), 'note_order_change fires on delete only when total > 0');
select is((select count(*) from pg_trigger t where t.tgfoid = '"svetra_holes"."note_order_change__fire"()'::regprocedure and not t.tgisinternal), 2::bigint, 'nothing else fires note_order_change');

-- Test data, inserted as the database owner
select lives_ok($svetra$alter table "public"."order" disable trigger "svetra_notify_paid_update"$svetra$, 'svetra_notify_paid_update is switched off while the test data goes in');
select lives_ok($svetra$alter table "public"."order" disable trigger "svetra_note_order_change_insert"$svetra$, 'svetra_note_order_change_insert is switched off while the test data goes in');
select lives_ok($svetra$alter table "public"."order" disable trigger "svetra_note_order_change_delete"$svetra$, 'svetra_note_order_change_delete is switched off while the test data goes in');
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test'), ('5e7a0000-0000-4000-8000-000000000003', 'carol@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8000-000000000001', 'svetra a')$svetra$, 'test data: Customer a');
select lives_ok($svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8000-000000000002', 'svetra b')$svetra$, 'test data: Customer b');
select lives_ok($svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000a', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', 'open', 1)$svetra$, 'test data: Order a');
select lives_ok($svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000b', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', 'paid', 2)$svetra$, 'test data: Order b');
select lives_ok($svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000a', '5e7a0000-0000-4000-8000-000000000001', 'svetra a')$svetra$, 'test data: Notification a');
select lives_ok($svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000b', '5e7a0000-0000-4000-8000-000000000002', 'svetra b')$svetra$, 'test data: Notification b');

-- Checks
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t see Customer a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t see Customer b');
select is(svetra_test.try('anon', null, $svetra$update "public"."customer" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t change Customer a');
select is(svetra_test.try('anon', null, $svetra$update "public"."customer" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t change Customer b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t delete Customer a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t delete Customer b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'anon can''t create Customer c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'anon can''t create Customer d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'alice can see Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t see Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."customer" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t change Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."customer" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t change Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t delete Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t delete Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'allowed', 'alice can create Customer c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'alice can''t create Customer d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."customer" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t set the owner of Customer a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."customer" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t set the owner of Customer b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t see Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'allowed', 'bob can see Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."customer" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t change Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."customer" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t change Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t delete Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t delete Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'bob can''t create Customer c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'allowed', 'bob can create Customer d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."customer" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t set the owner of Customer a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."customer" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t set the owner of Customer b to alice');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t see Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t see Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."customer" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."customer" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."customer" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$, 'aal2'), 'denied', 'carol can''t create Customer c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$, 'aal2'), 'denied', 'carol can''t create Customer d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."customer" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Customer a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."customer" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Customer b to themselves');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t see Order a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t see Order b');
select is(svetra_test.try('anon', null, $svetra$update "public"."order" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t change Order a');
select is(svetra_test.try('anon', null, $svetra$update "public"."order" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t change Order b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t delete Order a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t delete Order b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', 'shipped', 3)$svetra$), 'denied', 'anon can''t create Order c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', 'open', 4)$svetra$), 'denied', 'anon can''t create Order d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can see Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t see Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."order" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can change Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."order" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t change Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can delete Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t delete Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', 'shipped', 3)$svetra$), 'allowed', 'alice can create Order c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', 'open', 4)$svetra$), 'denied', 'alice can''t create Order d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t set the owner of Order a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."order" set "customer" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can point the customer of Order a at Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t set the owner of Order b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."order" set "customer" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t point the customer of Order b at Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000002', "customer" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t change the owner and the customer of Order a together (point it at Customer b)');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000001', "customer" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t change the owner and the customer of Order b together (point it at Customer a)');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t see Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can see Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."order" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t change Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."order" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can change Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t delete Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can delete Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', 'shipped', 3)$svetra$), 'denied', 'bob can''t create Order c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', 'open', 4)$svetra$), 'allowed', 'bob can create Order d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t set the owner of Order a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."order" set "customer" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t point the customer of Order a at Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t set the owner of Order b to alice');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."order" set "customer" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can point the customer of Order b at Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000002', "customer" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t change the owner and the customer of Order a together (point it at Customer b)');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000001', "customer" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t change the owner and the customer of Order b together (point it at Customer a)');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t see Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t see Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."order" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."order" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Order a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."order" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Order b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', 'shipped', 3)$svetra$, 'aal2'), 'denied', 'carol can''t create Order c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8000-000000000002', '5e7a0000-0000-4000-8001-00010000000b', 'open', 4)$svetra$, 'aal2'), 'denied', 'carol can''t create Order d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Order a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."order" set "customer" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t point the customer of Order a at Customer b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Order b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."order" set "customer" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t point the customer of Order b at Customer a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000003', "customer" = '5e7a0000-0000-4000-8001-00010000000b' where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change the owner and the customer of Order a together (point it at Customer b)');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."order" set "owner" = '5e7a0000-0000-4000-8000-000000000003', "customer" = '5e7a0000-0000-4000-8001-00010000000a' where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change the owner and the customer of Order b together (point it at Customer a)');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'anon can''t see Notification a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'anon can''t see Notification b');
select is(svetra_test.try('anon', null, $svetra$update "public"."notification" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'anon can''t change Notification a');
select is(svetra_test.try('anon', null, $svetra$update "public"."notification" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'anon can''t change Notification b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'anon can''t delete Notification a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'anon can''t delete Notification b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'anon can''t create Notification c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'anon can''t create Notification d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'allowed', 'alice can see Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'alice can''t see Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."notification" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'alice can''t change Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."notification" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'alice can''t change Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'alice can''t delete Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'alice can''t delete Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'alice can''t create Notification c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'alice can''t create Notification d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."notification" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'alice can''t set the owner of Notification a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."notification" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'alice can''t set the owner of Notification b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'bob can''t see Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'allowed', 'bob can see Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."notification" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'bob can''t change Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."notification" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'bob can''t change Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'bob can''t delete Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'bob can''t delete Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'bob can''t create Notification c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$), 'denied', 'bob can''t create Notification d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."notification" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$), 'denied', 'bob can''t set the owner of Notification a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."notification" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$), 'denied', 'bob can''t set the owner of Notification b to alice');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$, 'aal2'), 'denied', 'carol can''t see Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$, 'aal2'), 'denied', 'carol can''t see Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."notification" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."notification" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Notification a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."notification" where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Notification b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$, 'aal2'), 'denied', 'carol can''t create Notification c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."notification" ("id", "owner", "message") values ('5e7a0000-0000-4000-8001-00030000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d')$svetra$, 'aal2'), 'denied', 'carol can''t create Notification d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."notification" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00030000000a'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Notification a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."notification" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00030000000b'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Notification b to themselves');
select is(svetra_test.privileges('svetra_hole_notify_paid', '"public"."customer"'), 'select', 'notify_paid can only read Customer');
select is(svetra_test.privileges('svetra_hole_notify_paid', '"public"."order"'), 'select', 'notify_paid can only read Order');
select is(svetra_test.privileges('svetra_hole_notify_paid', '"public"."notification"'), 'select, insert, update, delete', 'notify_paid can change Notification');
select is(svetra_test.try('svetra_hole_notify_paid', null, $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'notify_paid can''t create a row of Customer');
select is(svetra_test.privileges('svetra_hole_note_order_change', '"public"."customer"'), 'none', 'note_order_change can''t touch Customer');
select is(svetra_test.privileges('svetra_hole_note_order_change', '"public"."order"'), 'select', 'note_order_change can only read Order');
select is(svetra_test.privileges('svetra_hole_note_order_change', '"public"."notification"'), 'select, insert, update, delete', 'note_order_change can change Notification');
select is(svetra_test.try('svetra_hole_note_order_change', null, $svetra$insert into "public"."customer" ("id", "owner", "name") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c')$svetra$), 'denied', 'note_order_change can''t create a row of Customer');
select lives_ok($svetra$alter table "public"."order" enable trigger "svetra_notify_paid_update"$svetra$, 'svetra_notify_paid_update is switched on again');
select lives_ok($svetra$alter table "public"."order" enable trigger "svetra_note_order_change_insert"$svetra$, 'svetra_note_order_change_insert is switched on again');
select lives_ok($svetra$alter table "public"."order" enable trigger "svetra_note_order_change_delete"$svetra$, 'svetra_note_order_change_delete is switched on again');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."order" ("id", "owner", "customer", "status", "total") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8001-00010000000a', 'shipped', 3)$svetra$, 'aal1', $svetra$create or replace function "svetra_holes"."note_order_change"(p_caller uuid, p_old "public"."order", p_new "public"."order") returns void language plpgsql security definer set search_path = '' as $probe$ begin raise exception 'svetra_fired' using errcode = 'SV001'; end $probe$$svetra$), 'error SV001: svetra_fired', 'alice creating Order c fires note_order_change');

select * from finish();
rollback;
