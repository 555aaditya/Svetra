-- Tables
create table "public"."visit" (
  "id" uuid primary key default gen_random_uuid(),
  "patient" uuid not null,
  "notes" text
);

-- Foreign keys
alter table "public"."visit" add constraint "visit_patient_fkey" foreign key ("patient") references "auth"."users" ("id");

-- Indexes
create index "visit_patient_idx" on "public"."visit" ("patient");

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
      and (g."role" not in ('admin') or coalesce((select auth.jwt() ->> 'aal'), '') = 'aal2')
  ) $$;
revoke all on function "svetra"."has_role"(text, uuid) from public;
grant usage on schema "svetra" to anon, authenticated;
grant execute on function "svetra"."has_role"(text, uuid) to anon, authenticated;

-- Access
alter table "public"."visit" enable row level security;

-- Policies
create policy "patients_see_own_select" on "public"."visit"
  for select to authenticated
  using ("patient" = (select auth.uid()));

create policy "patients_see_own_insert" on "public"."visit"
  for insert to authenticated
  with check ("patient" = (select auth.uid()));

create policy "admins_manage_select" on "public"."visit"
  for select to authenticated
  using ((select "svetra"."has_role"('admin')));

create policy "admins_manage_update" on "public"."visit"
  for update to authenticated
  using ((select "svetra"."has_role"('admin')))
  with check ((select "svetra"."has_role"('admin')));

create policy "admins_manage_delete" on "public"."visit"
  for delete to authenticated
  using ((select "svetra"."has_role"('admin')));
