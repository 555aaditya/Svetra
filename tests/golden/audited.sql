-- Tables
create table "public"."note" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "body" text
);

-- Foreign keys
alter table "public"."note" add constraint "note_owner_fkey" foreign key ("owner") references "auth"."users" ("id");

-- Indexes
create index "note_owner_idx" on "public"."note" ("owner");

-- Access
alter table "public"."note" enable row level security;

-- Policies
create policy "owner_all_select" on "public"."note"
  for select to authenticated
  using ("owner" = (select auth.uid()));

create policy "owner_all_insert" on "public"."note"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

create policy "owner_all_update" on "public"."note"
  for update to authenticated
  using ("owner" = (select auth.uid()))
  with check ("owner" = (select auth.uid()));

create policy "owner_all_delete" on "public"."note"
  for delete to authenticated
  using ("owner" = (select auth.uid()));

-- Audit
create schema if not exists "svetra";
create table "svetra"."audit_log" (
  "id" bigint generated always as identity primary key,
  "at" timestamptz not null default now(),
  "actor" uuid,
  "kind" text not null check ("kind" in ('row', 'call')),
  "subject" text not null,
  "action" text not null check ("action" in ('insert', 'update', 'delete', 'call')),
  "old" jsonb,
  "new" jsonb
);
alter table "svetra"."audit_log" enable row level security;
revoke all on table "svetra"."audit_log" from public, anon, authenticated;

create function "svetra"."audit_frozen"() returns trigger language plpgsql as $svetra$
begin
  raise exception 'the audit log is append-only' using errcode = '42501';
end $svetra$;
create trigger "svetra_audit_frozen" before update or delete on "svetra"."audit_log"
  for each row execute function "svetra"."audit_frozen"();
create trigger "svetra_audit_frozen_truncate" before truncate on "svetra"."audit_log"
  for each statement execute function "svetra"."audit_frozen"();

create function "svetra"."audit_row"() returns trigger language plpgsql
  security definer set search_path = '' as $svetra$
begin
  insert into "svetra"."audit_log" ("actor", "kind", "subject", "action", "old", "new")
  values ((select auth.uid()), 'row', tg_table_name, lower(tg_op),
    case when tg_op <> 'INSERT' then to_jsonb(old) end,
    case when tg_op <> 'DELETE' then to_jsonb(new) end);
  return null;
end $svetra$;
create trigger "svetra_audit" after insert or update or delete on "public"."note"
  for each row execute function "svetra"."audit_row"();

-- Holes
create schema if not exists "svetra_holes";

do $$ begin create role "svetra_hole_archive" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_archive" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_archive";
grant usage on schema "public" to "svetra_hole_archive";
grant select, insert, update, delete on table "public"."note" to "svetra_hole_archive";
create policy "svetra_hole_archive" on "public"."note" for all to "svetra_hole_archive" using (true) with check (true);
create function "svetra_holes"."archive"(p_caller uuid, p_note uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce((select t."owner" from "public"."note" t where t."id" = p_note) = p_caller, false) then
    raise exception 'not allowed to call archive' using errcode = '42501';
  end if;
  raise exception 'archive is not written yet: edit supabase/holes/archive.sql';
end
$svetra$;
alter function "svetra_holes"."archive"(uuid, uuid) owner to "svetra_hole_archive";
revoke all on function "svetra_holes"."archive"(uuid, uuid) from public, anon, authenticated;
grant execute on function "svetra_holes"."archive"(uuid, uuid) to current_user;
create function "public"."archive"(note uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin insert into "svetra"."audit_log" ("actor", "kind", "subject", "action", "new") values ((select auth.uid()), 'call', 'archive', 'call', jsonb_build_object('note', note)); perform "svetra_holes"."archive"((select auth.uid()), note); end $svetra$;
revoke all on function "public"."archive"(uuid) from public, anon, authenticated;
grant execute on function "public"."archive"(uuid) to authenticated;

do $$ begin create role "svetra_hole_sweep" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_sweep" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_sweep";
grant usage on schema "public" to "svetra_hole_sweep";
grant select, insert, update, delete on table "public"."note" to "svetra_hole_sweep";
create policy "svetra_hole_sweep" on "public"."note" for all to "svetra_hole_sweep" using (true) with check (true);
create function "svetra_holes"."sweep"(p_caller uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'sweep is not written yet: edit supabase/holes/sweep.sql';
end
$svetra$;
alter function "svetra_holes"."sweep"(uuid) owner to "svetra_hole_sweep";
revoke all on function "svetra_holes"."sweep"(uuid) from public, anon, authenticated;
grant execute on function "svetra_holes"."sweep"(uuid) to current_user;
create function "public"."sweep"()
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin insert into "svetra"."audit_log" ("actor", "kind", "subject", "action", "new") values ((select auth.uid()), 'call', 'sweep', 'call', '{}'::jsonb); perform "svetra_holes"."sweep"((select auth.uid())); end $svetra$;
revoke all on function "public"."sweep"() from public, anon, authenticated;
grant execute on function "public"."sweep"() to authenticated;
