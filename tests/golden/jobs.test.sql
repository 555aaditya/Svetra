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

select plan(49);

-- Test data, inserted as the database owner
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test'), ('5e7a0000-0000-4000-8000-000000000003', 'carol@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8000-000000000001', 'svetra a', 'pending', '2030-01-01T10:00:00Z')$svetra$, 'test data: Invite a');
select lives_ok($svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8000-000000000002', 'svetra b', 'accepted', '2030-01-02T10:00:00Z')$svetra$, 'test data: Invite b');

-- Checks
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t see Invite a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t see Invite b');
select is(svetra_test.try('anon', null, $svetra$update "public"."invite" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t change Invite a');
select is(svetra_test.try('anon', null, $svetra$update "public"."invite" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t change Invite b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t delete Invite a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t delete Invite b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c', 'expired', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'anon can''t create Invite c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d', 'pending', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'anon can''t create Invite d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'alice can see Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t see Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."invite" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t change Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."invite" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t change Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t delete Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t delete Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c', 'expired', '2030-01-03T10:00:00Z')$svetra$), 'allowed', 'alice can create Invite c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d', 'pending', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'alice can''t create Invite d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."invite" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t set the owner of Invite a to bob');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."invite" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t set the owner of Invite b to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t see Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'allowed', 'bob can see Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."invite" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t change Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."invite" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t change Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t delete Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t delete Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c', 'expired', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'bob can''t create Invite c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d', 'pending', '2030-01-04T10:00:00Z')$svetra$), 'allowed', 'bob can create Invite d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."invite" set "owner" = '5e7a0000-0000-4000-8000-000000000002' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t set the owner of Invite a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."invite" set "owner" = '5e7a0000-0000-4000-8000-000000000001' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t set the owner of Invite b to alice');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t see Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$select 1 from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t see Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."invite" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t change Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."invite" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t change Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t delete Invite a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$delete from "public"."invite" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t delete Invite b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'svetra c', 'expired', '2030-01-03T10:00:00Z')$svetra$, 'aal2'), 'denied', 'carol can''t create Invite c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$insert into "public"."invite" ("id", "owner", "email", "status", "expires_at") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'svetra d', 'pending', '2030-01-04T10:00:00Z')$svetra$, 'aal2'), 'denied', 'carol can''t create Invite d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."invite" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Invite a to themselves');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000003', $svetra$update "public"."invite" set "owner" = '5e7a0000-0000-4000-8000-000000000003' where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$, 'aal2'), 'denied', 'carol can''t set the owner of Invite b to themselves');
select is(svetra_test.privileges('svetra_hole_expire_invites', '"public"."invite"'), 'select, insert, update, delete', 'expire_invites can change Invite');
select is((select count(*) from pg_roles r, pg_proc p join pg_namespace n on n.oid = p.pronamespace where r.rolname in ('anon', 'authenticated', 'service_role') and n.nspname = 'svetra_holes' and p.proname = 'expire_invites' and has_function_privilege(r.rolname, p.oid, 'execute')), 0::bigint, 'no one calling the API can run expire_invites, or pass it a caller');
select is((select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace where n.nspname = 'public' and p.proname = 'expire_invites'), 0::bigint, 'expire_invites has no function the app can call');
select is((select count(*) from cron.job where jobname = 'svetra_expire_invites'), 1::bigint, 'expire_invites has exactly one scheduled job');
select is((select schedule from cron.job where jobname = 'svetra_expire_invites'), '0 3 * * *', 'expire_invites runs on the schedule 0 3 * * *');
select ok(coalesce((select position('"svetra_holes"."expire_invites"(null)' in command) from cron.job where jobname = 'svetra_expire_invites'), 0) > 0, 'the job of expire_invites runs expire_invites');
select is((select count(*) from cron.job where jobname like 'svetra\_%' and jobname not in ('svetra_expire_invites')), 0::bigint, 'no scheduled job is left over from code that was removed');
select ok(coalesce(current_setting('cron.timezone', true), 'UTC') in ('UTC', 'GMT'), 'scheduled times are in UTC');

select * from finish();
rollback;
