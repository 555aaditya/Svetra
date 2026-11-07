-- Tables
create table "public"."audit_log" (
  "id" uuid primary key default gen_random_uuid(),
  "note" text
);

create table "public"."post" (
  "id" uuid primary key default gen_random_uuid(),
  "author" uuid not null,
  "body" text
);

-- Foreign keys
alter table "public"."post" add constraint "post_author_fkey" foreign key ("author") references "auth"."users" ("id");

-- Indexes
create index "post_author_idx" on "public"."post" ("author");

-- Access
alter table "public"."audit_log" enable row level security;
revoke all on table "public"."audit_log" from anon, authenticated;
alter table "public"."post" enable row level security;

-- Policies
create policy "posts_by_author_select" on "public"."post"
  for select to authenticated
  using ("author" = (select auth.uid()));

create policy "posts_by_author_insert" on "public"."post"
  for insert to authenticated
  with check ("author" = (select auth.uid()));

create policy "posts_by_author_update" on "public"."post"
  for update to authenticated
  using ("author" = (select auth.uid()))
  with check ("author" = (select auth.uid()));

create policy "posts_by_author_delete" on "public"."post"
  for delete to authenticated
  using ("author" = (select auth.uid()));

create policy "public_read" on "public"."post"
  for select to anon, authenticated
  using (true);

-- Holes
create schema if not exists "svetra_holes";

create role "svetra_hole_write_audit" nologin;
grant "svetra_hole_write_audit" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_write_audit";
grant usage on schema "public" to "svetra_hole_write_audit";
grant select, insert, update, delete on table "public"."audit_log" to "svetra_hole_write_audit";
create policy "svetra_hole_write_audit" on "public"."audit_log" for all to "svetra_hole_write_audit" using (true) with check (true);
create function "svetra_holes"."write_audit"(p_caller uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'write_audit is not written yet: edit supabase/holes/write_audit.sql';
end
$svetra$;
alter function "svetra_holes"."write_audit"(uuid) owner to "svetra_hole_write_audit";
revoke all on function "svetra_holes"."write_audit"(uuid) from public;
grant execute on function "svetra_holes"."write_audit"(uuid) to current_user;
create function "public"."write_audit"()
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin perform "svetra_holes"."write_audit"((select auth.uid())); end $svetra$;
revoke all on function "public"."write_audit"() from public;
grant execute on function "public"."write_audit"() to authenticated;
