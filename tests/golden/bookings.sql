-- Tables
create table "public"."location" (
  "id" uuid primary key default gen_random_uuid(),
  "name" text not null
);

create table "public"."booking" (
  "id" uuid primary key default gen_random_uuid(),
  "customer" uuid not null,
  "location" uuid not null,
  "starts" timestamptz not null
);

-- Foreign keys
alter table "public"."booking" add constraint "booking_customer_fkey" foreign key ("customer") references "auth"."users" ("id");
alter table "public"."booking" add constraint "booking_location_fkey" foreign key ("location") references "public"."location" ("id");

-- Indexes
create index "booking_customer_idx" on "public"."booking" ("customer");
create index "booking_location_idx" on "public"."booking" ("location");

-- Roles
create schema if not exists "svetra";
create table "svetra"."role_grants" (
  "user_id" uuid not null references "auth"."users" ("id") on delete cascade,
  "role" text not null check ("role" in ('admin', 'staff')),
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
alter table "public"."location" enable row level security;
alter table "public"."booking" enable row level security;

-- Policies
create policy "customers_see_own_select" on "public"."booking"
  for select to authenticated
  using ("customer" = (select auth.uid()));

create policy "customers_see_own_insert" on "public"."booking"
  for insert to authenticated
  with check ("customer" = (select auth.uid()));

create policy "staff_manage_local_select" on "public"."booking"
  for select to authenticated
  using ("svetra"."has_role"('staff', "location"));

create policy "staff_manage_local_update" on "public"."booking"
  for update to authenticated
  using ("svetra"."has_role"('staff', "location"))
  with check ("svetra"."has_role"('staff', "location"));

create policy "admins_manage_all_select" on "public"."booking"
  for select to authenticated
  using ((select "svetra"."has_role"('admin')));

create policy "admins_manage_all_update" on "public"."booking"
  for update to authenticated
  using ((select "svetra"."has_role"('admin')))
  with check ((select "svetra"."has_role"('admin')));

create policy "admins_manage_all_delete" on "public"."booking"
  for delete to authenticated
  using ((select "svetra"."has_role"('admin')));

create policy "staff_see_location" on "public"."location"
  for select to authenticated
  using ("svetra"."has_role"('staff', "id"));

create policy "admins_see_locations_select" on "public"."location"
  for select to authenticated
  using ((select "svetra"."has_role"('admin')));

create policy "admins_see_locations_insert" on "public"."location"
  for insert to authenticated
  with check ((select "svetra"."has_role"('admin')));

create policy "admins_see_locations_update" on "public"."location"
  for update to authenticated
  using ((select "svetra"."has_role"('admin')))
  with check ((select "svetra"."has_role"('admin')));
