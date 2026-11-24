-- Tables
create table "public"."invite" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "email" text not null,
  "status" text default 'pending',
  "expires_at" timestamptz not null,
  constraint "invite_status_values" check ("status" in ('pending', 'accepted', 'expired'))
);

-- Foreign keys
alter table "public"."invite" add constraint "invite_owner_fkey" foreign key ("owner") references "auth"."users" ("id");

-- Indexes
create index "invite_owner_idx" on "public"."invite" ("owner");

-- Access
alter table "public"."invite" enable row level security;

-- Policies
create policy "owner_reads_own" on "public"."invite"
  for select to authenticated
  using ("owner" = (select auth.uid()));

create policy "owner_invites" on "public"."invite"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

-- Holes
create schema if not exists "svetra_holes";
do $$ begin if to_regnamespace('cron') is null then create extension pg_cron; end if; end $$;

do $$ begin create role "svetra_hole_expire_invites" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_expire_invites" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_expire_invites";
grant usage on schema "public" to "svetra_hole_expire_invites";
grant select, insert, update, delete on table "public"."invite" to "svetra_hole_expire_invites";
create policy "svetra_hole_expire_invites" on "public"."invite" for all to "svetra_hole_expire_invites" using (true) with check (true);
create function "svetra_holes"."expire_invites"(p_caller uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'expire_invites is not written yet: edit supabase/holes/expire_invites.sql';
end
$svetra$;
alter function "svetra_holes"."expire_invites"(uuid) owner to "svetra_hole_expire_invites";
revoke all on function "svetra_holes"."expire_invites"(uuid) from public, anon, authenticated;
grant execute on function "svetra_holes"."expire_invites"(uuid) to current_user;
do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on function "svetra_holes"."expire_invites"(uuid) from service_role; end if; end $$;
select cron.schedule('svetra_expire_invites', '0 3 * * *',
  $job$select "svetra_holes"."expire_invites"(null)
    where pg_try_advisory_xact_lock(hashtext('svetra_expire_invites'))$job$);
select cron.unschedule(jobid) from cron.job
  where jobname like 'svetra\_%' and jobname not in ('svetra_expire_invites');
