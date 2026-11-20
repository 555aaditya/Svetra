-- More of Supabase than src/standin/base.sql has, because real migrations assume it: the
-- service role, the extensions and storage schemas, auth.role(), the realtime publication.
-- Applied after the base stand-in, after the base. Not the real thing: extensions
-- that don't ship with the stock PostgreSQL image (pg_net, pg_cron, vector) still fail, and the
-- harness counts those migrations as failed.
create role service_role nologin bypassrls;
create role authenticator nologin;
create role supabase_admin nologin;
create role supabase_auth_admin nologin;
create role supabase_storage_admin nologin;
create role dashboard_user nologin;

create schema extensions;
grant usage on schema extensions to anon, authenticated, service_role;
create extension if not exists "uuid-ossp" schema extensions;
create extension if not exists pgcrypto schema extensions;
-- Supabase's default search path, so unqualified uuid_generate_v4() and friends resolve.
do $$ begin execute format('alter database %I set search_path = "$user", public, extensions', current_database()); end $$;

grant usage on schema public, auth to service_role;
grant all on all tables in schema public to service_role;
alter default privileges in schema public grant all on tables to service_role;
alter default privileges in schema public grant all on functions to service_role;
alter default privileges in schema public grant all on sequences to anon, authenticated, service_role;

create function auth.role() returns text language sql stable
  as $$ select nullif(current_setting('request.jwt.claim.role', true), '')::text $$;
create function auth.email() returns text language sql stable
  as $$ select nullif(current_setting('request.jwt.claim.email', true), '')::text $$;
grant execute on function auth.role(), auth.email() to anon, authenticated, service_role;

create schema storage;
create table storage.buckets (
  id text primary key, name text not null, owner uuid, public boolean default false,
  file_size_limit bigint, allowed_mime_types text[],
  created_at timestamptz default now(), updated_at timestamptz default now());
create table storage.objects (
  id uuid primary key default extensions.uuid_generate_v4(), bucket_id text references storage.buckets (id),
  name text, owner uuid, metadata jsonb, path_tokens text[],
  created_at timestamptz default now(), updated_at timestamptz default now(), last_accessed_at timestamptz);
alter table storage.buckets enable row level security;
alter table storage.objects enable row level security;
create function storage.foldername(name text) returns text[] language sql immutable
  as $$ select (string_to_array(name, '/'))[1:greatest(array_length(string_to_array(name, '/'), 1) - 1, 0)] $$;
create function storage.filename(name text) returns text language sql immutable
  as $$ select (string_to_array(name, '/'))[array_length(string_to_array(name, '/'), 1)] $$;
grant usage on schema storage to anon, authenticated, service_role;
grant all on all tables in schema storage to anon, authenticated, service_role;

create schema realtime;
create publication supabase_realtime;
create schema graphql_public;
