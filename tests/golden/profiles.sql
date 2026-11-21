-- Tables
create table "public"."profile" (
  "id" uuid primary key,
  "username" text not null,
  "status" text default 'OFFLINE',
  constraint "profile_status_values" check ("status" in ('ONLINE', 'OFFLINE'))
);

create table "public"."note" (
  "id" uuid primary key default gen_random_uuid(),
  "author" uuid not null,
  "body" text
);

-- Foreign keys
alter table "public"."profile" add constraint "profile_id_fkey" foreign key ("id") references "auth"."users" ("id");
alter table "public"."note" add constraint "note_author_fkey" foreign key ("author") references "auth"."users" ("id");

-- Indexes
create index "note_author_idx" on "public"."note" ("author");

-- Roles
create schema if not exists "svetra";
create table "svetra"."role_grants" (
  "user_id" uuid not null references "auth"."users" ("id") on delete cascade,
  "role" text not null check ("role" in ('admin')),
  "scope_id" uuid,
  constraint "role_grants_key" unique nulls not distinct ("user_id", "role", "scope_id")
);
alter table "svetra"."role_grants" enable row level security;

create function "svetra"."has_role"(p_role text, p_scope uuid default null)
  returns boolean language sql stable security definer set search_path = ''
  as $$ select exists (
    select 1 from "svetra"."role_grants" g
    where g."user_id" = (select auth.uid()) and g."role" = p_role and g."scope_id" is not distinct from p_scope
  ) $$;
revoke all on function "svetra"."has_role"(text, uuid) from public;
grant usage on schema "svetra" to anon, authenticated;
grant execute on function "svetra"."has_role"(text, uuid) to anon, authenticated;

-- Access
alter table "public"."profile" enable row level security;
alter table "public"."note" enable row level security;

-- Policies
create policy "profiles_read" on "public"."profile"
  for select to authenticated
  using (true);

create policy "profiles_insert_own" on "public"."profile"
  for insert to authenticated
  with check ("id" = (select auth.uid()));

create policy "profiles_update_own" on "public"."profile"
  for update to authenticated
  using ("id" = (select auth.uid()))
  with check ("id" = (select auth.uid()));

create policy "profiles_delete_own" on "public"."profile"
  for delete to authenticated
  using ("id" = (select auth.uid()));

create policy "profiles_delete_admin" on "public"."profile"
  for delete to authenticated
  using ((select "svetra"."has_role"('admin')));

create policy "notes_own_select" on "public"."note"
  for select to authenticated
  using ("author" = (select auth.uid()));

create policy "notes_own_insert" on "public"."note"
  for insert to authenticated
  with check ("author" = (select auth.uid()));

create policy "notes_own_update" on "public"."note"
  for update to authenticated
  using ("author" = (select auth.uid()))
  with check ("author" = (select auth.uid()));

create policy "notes_own_delete" on "public"."note"
  for delete to authenticated
  using ("author" = (select auth.uid()));
