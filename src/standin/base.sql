-- A minimal stand-in for Supabase: the auth schema, auth.uid() and auth.jwt() from the JWT claims,
-- the anon and authenticated API roles, and Supabase's default grants on public tables.
create schema auth;
create table auth.users (id uuid primary key, email text);
create function auth.uid() returns uuid language sql stable
  as $$ select nullif(current_setting('request.jwt.claim.sub', true), '')::uuid $$;
create role anon nologin;
create role authenticated nologin;
-- The role Supabase's keys of that name use: the only caller a webhook's database function has.
create role service_role nologin bypassrls;
grant usage on schema public to service_role;
create function auth.jwt() returns jsonb language sql stable
  as $$ select coalesce(nullif(current_setting('request.jwt.claims', true), ''), '{}')::jsonb $$;
grant usage on schema public, auth to anon, authenticated;
grant execute on function auth.uid(), auth.jwt() to anon, authenticated;
alter default privileges in schema public grant all on tables to anon, authenticated;
-- Supabase also lets anon and authenticated run every new function in public, directly rather
-- than through PUBLIC, so `revoke ... from public` alone doesn't take it away.
alter default privileges in schema public grant all on functions to anon, authenticated;
-- pg_cron, as far as generated schedules use it: a job table, schedule (create or replace by
-- name) and unschedule. Nothing runs the jobs. The generated SQL creates the real extension
-- only when this schema is missing, so on plain PostgreSQL it never needs one.
create schema cron;
create table cron.job (
  jobid bigserial primary key, schedule text not null, command text not null,
  nodename text not null default 'localhost', nodeport int not null default 5432,
  database text not null default current_database(), username text not null default current_user,
  active boolean not null default true, jobname text unique);
create function cron.schedule(job_name text, schedule text, command text) returns bigint language sql as $$
  insert into cron.job (jobname, schedule, command) values (job_name, schedule, command)
  on conflict (jobname) do update set schedule = excluded.schedule, command = excluded.command
  returning jobid $$;
create function cron.unschedule(job_id bigint) returns boolean language sql as $$
  with gone as (delete from cron.job where jobid = job_id returning 1) select exists (select from gone) $$;
create function cron.unschedule(job_name text) returns boolean language sql as $$
  with gone as (delete from cron.job where jobname = job_name returning 1) select exists (select from gone) $$;
