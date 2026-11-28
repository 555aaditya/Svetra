-- Tables
create table "public"."invoice" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "amount" numeric(12, 2) not null,
  "status" text default 'open',
  constraint "invoice_status_values" check ("status" in ('open', 'paid'))
);

create table "public"."payment" (
  "id" uuid primary key default gen_random_uuid(),
  "invoice" uuid not null,
  "amount" numeric(12, 2) not null
);

-- Foreign keys
alter table "public"."invoice" add constraint "invoice_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."payment" add constraint "payment_invoice_fkey" foreign key ("invoice") references "public"."invoice" ("id");

-- Indexes
create index "invoice_owner_idx" on "public"."invoice" ("owner");
create index "payment_invoice_idx" on "public"."payment" ("invoice");

-- Access
alter table "public"."invoice" enable row level security;
alter table "public"."payment" enable row level security;

-- Policies
create policy "own_invoices" on "public"."invoice"
  for select to authenticated
  using ("owner" = (select auth.uid()));

create policy "own_payments" on "public"."payment"
  for select to authenticated
  using ((select t."owner" from "public"."invoice" t where t."id" = "payment"."invoice") = (select auth.uid()));

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

-- Holes
create schema if not exists "svetra_holes";
create schema if not exists "svetra";
create table "svetra"."webhook_events" (
  "hook" text not null,
  "event_id" text not null,
  primary key ("hook", "event_id")
);
alter table "svetra"."webhook_events" enable row level security;
revoke all on table "svetra"."webhook_events" from public, anon, authenticated;
do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on table "svetra"."webhook_events" from service_role; end if; end $$;

do $$ begin create role "svetra_hole_payment_received" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_payment_received" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_payment_received";
grant usage on schema "public" to "svetra_hole_payment_received";
grant select, insert, update, delete on table "public"."invoice" to "svetra_hole_payment_received";
create policy "svetra_hole_payment_received" on "public"."invoice" for all to "svetra_hole_payment_received" using (true) with check (true);
grant select, insert, update, delete on table "public"."payment" to "svetra_hole_payment_received";
create policy "svetra_hole_payment_received" on "public"."payment" for all to "svetra_hole_payment_received" using (true) with check (true);
create function "svetra_holes"."payment_received"(p_caller uuid, p_invoice uuid, p_paid numeric(12, 2), p_event_id text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce(p_paid > 0, false) then
    raise exception 'not allowed to call payment_received' using errcode = '42501';
  end if;
  raise exception 'payment_received is not written yet: edit supabase/holes/payment_received.sql';
end
$svetra$;
alter function "svetra_holes"."payment_received"(uuid, uuid, numeric(12, 2), text) owner to "svetra_hole_payment_received";
revoke all on function "svetra_holes"."payment_received"(uuid, uuid, numeric(12, 2), text) from public, anon, authenticated;
grant execute on function "svetra_holes"."payment_received"(uuid, uuid, numeric(12, 2), text) to current_user;
do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on function "svetra_holes"."payment_received"(uuid, uuid, numeric(12, 2), text) from service_role; end if; end $$;
create function "public"."payment_received"(invoice uuid, paid numeric(12, 2), event_id text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin insert into "svetra"."webhook_events" ("hook", "event_id") values ('payment_received', (event_id)::text) on conflict do nothing; if not found then return; end if; insert into "svetra"."audit_log" ("actor", "kind", "subject", "action", "new") values ((select auth.uid()), 'call', 'payment_received', 'call', jsonb_build_object('invoice', invoice, 'paid', paid, 'event_id', event_id)); perform "svetra_holes"."payment_received"((select auth.uid()), invoice, paid, event_id); end $svetra$;
revoke all on function "public"."payment_received"(uuid, numeric(12, 2), text) from public, anon, authenticated;
grant execute on function "public"."payment_received"(uuid, numeric(12, 2), text) to service_role;

do $$ begin create role "svetra_hole_form_submitted" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_form_submitted" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_form_submitted";
grant usage on schema "public" to "svetra_hole_form_submitted";
grant select on table "public"."invoice" to "svetra_hole_form_submitted";
create policy "svetra_hole_form_submitted" on "public"."invoice" for select to "svetra_hole_form_submitted" using (true);
create function "svetra_holes"."form_submitted"(p_caller uuid, p_invoice uuid, p_note text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce(p_note <> '', false) then
    raise exception using message = 'form_submitted: note != ''''', errcode = '22023';
  end if;
  raise exception 'form_submitted is not written yet: edit supabase/holes/form_submitted.sql';
end
$svetra$;
alter function "svetra_holes"."form_submitted"(uuid, uuid, text) owner to "svetra_hole_form_submitted";
revoke all on function "svetra_holes"."form_submitted"(uuid, uuid, text) from public, anon, authenticated;
grant execute on function "svetra_holes"."form_submitted"(uuid, uuid, text) to current_user;
do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on function "svetra_holes"."form_submitted"(uuid, uuid, text) from service_role; end if; end $$;
create function "public"."form_submitted"(invoice uuid, note text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin perform "svetra_holes"."form_submitted"((select auth.uid()), invoice, note); end $svetra$;
revoke all on function "public"."form_submitted"(uuid, text) from public, anon, authenticated;
grant execute on function "public"."form_submitted"(uuid, text) to service_role;
