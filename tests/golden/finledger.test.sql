-- Access tests: run with `supabase test db`. Every check runs as a real user, never the
-- service key, and the whole file rolls back. People: anon, alice, bob.
begin;
create extension if not exists pgtap;
-- Run one statement as one person, then undo it. Returns allowed, denied or the error.
create schema svetra_test;
create function svetra_test.try(p_role text, p_user uuid, p_sql text) returns text
language plpgsql as $$
declare n bigint;
begin
  begin
    perform set_config('role', p_role, true);
    perform set_config('request.jwt.claim.sub', coalesce(p_user::text, ''), true);
    perform set_config('request.jwt.claims', json_build_object('sub', p_user, 'role', p_role)::text, true);
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

select plan(53);

-- Test data, inserted as the database owner
select lives_ok($svetra$insert into auth.users (id, email) values ('5e7a0000-0000-4000-8000-000000000001', 'alice@svetra.test'), ('5e7a0000-0000-4000-8000-000000000002', 'bob@svetra.test')$svetra$, 'test data: people');
select lives_ok($svetra$insert into "public"."account" ("id", "owner", "currency", "balance", "status", "created_at") values ('5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8000-000000000001', 'USD', 1, 'active', '2030-01-01T10:00:00Z')$svetra$, 'test data: Account a');
select lives_ok($svetra$insert into "public"."account" ("id", "owner", "currency", "balance", "status", "created_at") values ('5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8000-000000000002', 'EUR', 2, 'frozen', '2030-01-02T10:00:00Z')$svetra$, 'test data: Account b');
select lives_ok($svetra$insert into "public"."transaction" ("id", "source", "destination", "amount", "status", "created_at") values ('5e7a0000-0000-4000-8001-00020000000a', '5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b', 1, 'pending', '2030-01-01T10:00:00Z')$svetra$, 'test data: Transaction a');
select lives_ok($svetra$insert into "public"."transaction" ("id", "source", "destination", "amount", "status", "created_at") values ('5e7a0000-0000-4000-8001-00020000000b', '5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8001-00010000000a', 2, 'committed', '2030-01-02T10:00:00Z')$svetra$, 'test data: Transaction b');

-- Checks
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t see Account a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t see Account b');
select is(svetra_test.try('anon', null, $svetra$update "public"."account" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t change Account a');
select is(svetra_test.try('anon', null, $svetra$update "public"."account" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t change Account b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'anon can''t delete Account a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'anon can''t delete Account b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."account" ("id", "owner", "currency", "balance", "status", "created_at") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'GBP', 3, 'closed', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'anon can''t create Account c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."account" ("id", "owner", "currency", "balance", "status", "created_at") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'INR', 4, 'active', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'anon can''t create Account d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'allowed', 'alice can see Account a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t see Account b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."account" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t change Account a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."account" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t change Account b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'alice can''t delete Account a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'alice can''t delete Account b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."account" ("id", "owner", "currency", "balance", "status", "created_at") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'GBP', 3, 'closed', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'alice can''t create Account c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."account" ("id", "owner", "currency", "balance", "status", "created_at") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'INR', 4, 'active', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'alice can''t create Account d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t see Account a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'allowed', 'bob can see Account b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."account" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t change Account a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."account" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t change Account b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000a'$svetra$), 'denied', 'bob can''t delete Account a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."account" where "id" = '5e7a0000-0000-4000-8001-00010000000b'$svetra$), 'denied', 'bob can''t delete Account b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."account" ("id", "owner", "currency", "balance", "status", "created_at") values ('5e7a0000-0000-4000-8001-00010000000c', '5e7a0000-0000-4000-8000-000000000001', 'GBP', 3, 'closed', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'bob can''t create Account c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."account" ("id", "owner", "currency", "balance", "status", "created_at") values ('5e7a0000-0000-4000-8001-00010000000d', '5e7a0000-0000-4000-8000-000000000002', 'INR', 4, 'active', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'bob can''t create Account d');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t see Transaction a');
select is(svetra_test.try('anon', null, $svetra$select 1 from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t see Transaction b');
select is(svetra_test.try('anon', null, $svetra$update "public"."transaction" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t change Transaction a');
select is(svetra_test.try('anon', null, $svetra$update "public"."transaction" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t change Transaction b');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'anon can''t delete Transaction a');
select is(svetra_test.try('anon', null, $svetra$delete from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'anon can''t delete Transaction b');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."transaction" ("id", "source", "destination", "amount", "status", "created_at") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b', 3, 'failed', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'anon can''t create Transaction c');
select is(svetra_test.try('anon', null, $svetra$insert into "public"."transaction" ("id", "source", "destination", "amount", "status", "created_at") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8001-00010000000a', 4, 'reversed', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'anon can''t create Transaction d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'alice can see Transaction a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$select 1 from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'alice can see Transaction b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."transaction" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t change Transaction a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$update "public"."transaction" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t change Transaction b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'alice can''t delete Transaction a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$delete from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'alice can''t delete Transaction b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."transaction" ("id", "source", "destination", "amount", "status", "created_at") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b', 3, 'failed', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'alice can''t create Transaction c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000001', $svetra$insert into "public"."transaction" ("id", "source", "destination", "amount", "status", "created_at") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8001-00010000000a', 4, 'reversed', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'alice can''t create Transaction d');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'allowed', 'bob can see Transaction a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$select 1 from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'allowed', 'bob can see Transaction b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."transaction" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t change Transaction a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$update "public"."transaction" set "id" = "id" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t change Transaction b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000a'$svetra$), 'denied', 'bob can''t delete Transaction a');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$delete from "public"."transaction" where "id" = '5e7a0000-0000-4000-8001-00020000000b'$svetra$), 'denied', 'bob can''t delete Transaction b');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."transaction" ("id", "source", "destination", "amount", "status", "created_at") values ('5e7a0000-0000-4000-8001-00020000000c', '5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b', 3, 'failed', '2030-01-03T10:00:00Z')$svetra$), 'denied', 'bob can''t create Transaction c');
select is(svetra_test.try('authenticated', '5e7a0000-0000-4000-8000-000000000002', $svetra$insert into "public"."transaction" ("id", "source", "destination", "amount", "status", "created_at") values ('5e7a0000-0000-4000-8001-00020000000d', '5e7a0000-0000-4000-8001-00010000000b', '5e7a0000-0000-4000-8001-00010000000a', 4, 'reversed', '2030-01-04T10:00:00Z')$svetra$), 'denied', 'bob can''t create Transaction d');

select * from finish();
rollback;
