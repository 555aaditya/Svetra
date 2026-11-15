-- Tables
create table "public"."team" (
  "id" uuid primary key default gen_random_uuid(),
  "name" text not null
);

create table "public"."task" (
  "id" uuid primary key default gen_random_uuid(),
  "team" uuid not null,
  "author" uuid not null,
  "title" text not null
);

-- Foreign keys
alter table "public"."task" add constraint "task_team_fkey" foreign key ("team") references "public"."team" ("id");
alter table "public"."task" add constraint "task_author_fkey" foreign key ("author") references "auth"."users" ("id");

-- Indexes
create index "task_team_idx" on "public"."task" ("team");
create index "task_author_idx" on "public"."task" ("author");

-- Roles
create schema if not exists "svetra";
create table "svetra"."role_grants" (
  "user_id" uuid not null references "auth"."users" ("id") on delete cascade,
  "role" text not null check ("role" in ('admin', 'member')),
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
alter table "public"."team" enable row level security;
alter table "public"."task" enable row level security;

-- Policies
create policy "members_see_team" on "public"."team"
  for select to authenticated
  using ("svetra"."has_role"('member', "id"));

create policy "admins_see_teams" on "public"."team"
  for select to authenticated
  using ((select "svetra"."has_role"('admin')));

create policy "members_see_tasks" on "public"."task"
  for select to authenticated
  using ("svetra"."has_role"('member', "team"));

create policy "members_add_tasks" on "public"."task"
  for insert to authenticated
  with check ("svetra"."has_role"('member', "team") and "author" = (select auth.uid()));

create policy "authors_edit_tasks_update" on "public"."task"
  for update to authenticated
  using ("author" = (select auth.uid()) and "svetra"."has_role"('member', "team"))
  with check ("author" = (select auth.uid()) and "svetra"."has_role"('member', "team"));

create policy "authors_edit_tasks_delete" on "public"."task"
  for delete to authenticated
  using ("author" = (select auth.uid()) and "svetra"."has_role"('member', "team"));

-- Holes
create schema if not exists "svetra_holes";

do $$ begin create role "svetra_hole_start_team" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_start_team" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_start_team";
grant usage on schema "public" to "svetra_hole_start_team";
grant select, insert, update, delete on table "public"."team" to "svetra_hole_start_team";
create policy "svetra_hole_start_team" on "public"."team" for all to "svetra_hole_start_team" using (true) with check (true);
grant usage on schema "svetra" to "svetra_hole_start_team";
grant execute on function "svetra"."has_role"(text, uuid) to "svetra_hole_start_team";
grant select, insert, delete on table "svetra"."role_grants" to "svetra_hole_start_team";
create policy "svetra_hole_start_team" on "svetra"."role_grants" for all to "svetra_hole_start_team" using ("role" in ('member')) with check ("role" in ('member'));
create function "svetra_holes"."start_team"(p_caller uuid, p_name text)
  returns "public"."team" language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'start_team is not written yet: edit supabase/holes/start_team.sql';
end
$svetra$;
alter function "svetra_holes"."start_team"(uuid, text) owner to "svetra_hole_start_team";
revoke all on function "svetra_holes"."start_team"(uuid, text) from public, anon, authenticated;
grant execute on function "svetra_holes"."start_team"(uuid, text) to current_user;
create function "public"."start_team"(name text)
  returns "public"."team" language plpgsql security definer set search_path = ''
  as $svetra$ begin return "svetra_holes"."start_team"((select auth.uid()), name); end $svetra$;
revoke all on function "public"."start_team"(text) from public, anon, authenticated;
grant execute on function "public"."start_team"(text) to authenticated;

do $$ begin create role "svetra_hole_add_member" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_add_member" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_add_member";
grant usage on schema "public" to "svetra_hole_add_member";
grant usage on schema "svetra" to "svetra_hole_add_member";
grant execute on function "svetra"."has_role"(text, uuid) to "svetra_hole_add_member";
grant select, insert, delete on table "svetra"."role_grants" to "svetra_hole_add_member";
create policy "svetra_hole_add_member" on "svetra"."role_grants" for all to "svetra_hole_add_member" using ("role" in ('member')) with check ("role" in ('member'));
create function "svetra_holes"."add_member"(p_caller uuid, p_team uuid, p_person uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce("svetra"."has_role"('member', p_team), false) then
    raise exception 'not allowed to call add_member' using errcode = '42501';
  end if;
  raise exception 'add_member is not written yet: edit supabase/holes/add_member.sql';
end
$svetra$;
alter function "svetra_holes"."add_member"(uuid, uuid, uuid) owner to "svetra_hole_add_member";
revoke all on function "svetra_holes"."add_member"(uuid, uuid, uuid) from public, anon, authenticated;
grant execute on function "svetra_holes"."add_member"(uuid, uuid, uuid) to current_user;
create function "public"."add_member"(team uuid, person uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin perform "svetra_holes"."add_member"((select auth.uid()), team, person); end $svetra$;
revoke all on function "public"."add_member"(uuid, uuid) from public, anon, authenticated;
grant execute on function "public"."add_member"(uuid, uuid) to authenticated;
