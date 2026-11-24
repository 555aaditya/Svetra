-- Tables
create table "public"."customer" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "name" text not null
);

create table "public"."order" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "customer" uuid not null,
  "status" text default 'open',
  "total" numeric(18, 2) not null,
  constraint "order_status_values" check ("status" in ('open', 'paid', 'shipped'))
);

create table "public"."notification" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "message" text not null
);

-- Foreign keys
alter table "public"."customer" add constraint "customer_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."order" add constraint "order_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."order" add constraint "order_customer_fkey" foreign key ("customer") references "public"."customer" ("id");
alter table "public"."notification" add constraint "notification_owner_fkey" foreign key ("owner") references "auth"."users" ("id");

-- Indexes
create index "customer_owner_idx" on "public"."customer" ("owner");
create index "order_owner_idx" on "public"."order" ("owner");
create index "order_customer_idx" on "public"."order" ("customer");
create index "notification_owner_idx" on "public"."notification" ("owner");

-- Access
alter table "public"."customer" enable row level security;
alter table "public"."order" enable row level security;
alter table "public"."notification" enable row level security;

-- Policies
create policy "owner_reads_customers" on "public"."customer"
  for select to authenticated
  using ("owner" = (select auth.uid()));

create policy "owner_adds_customers" on "public"."customer"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

create policy "owner_reads_orders" on "public"."order"
  for select to authenticated
  using ("owner" = (select auth.uid()));

create policy "owner_places_orders" on "public"."order"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

create policy "owner_changes_orders" on "public"."order"
  for update to authenticated
  using ("owner" = (select auth.uid()))
  with check ("owner" = (select auth.uid()));

create policy "owner_deletes_orders" on "public"."order"
  for delete to authenticated
  using ("owner" = (select auth.uid()));

create policy "owner_reads_notifications" on "public"."notification"
  for select to authenticated
  using ("owner" = (select auth.uid()));

-- Holes
create schema if not exists "svetra_holes";

do $$ begin create role "svetra_hole_notify_paid" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_notify_paid" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_notify_paid";
grant usage on schema "public" to "svetra_hole_notify_paid";
grant select on table "public"."customer" to "svetra_hole_notify_paid";
create policy "svetra_hole_notify_paid" on "public"."customer" for select to "svetra_hole_notify_paid" using (true);
grant select on table "public"."order" to "svetra_hole_notify_paid";
create policy "svetra_hole_notify_paid" on "public"."order" for select to "svetra_hole_notify_paid" using (true);
grant select, insert, update, delete on table "public"."notification" to "svetra_hole_notify_paid";
create policy "svetra_hole_notify_paid" on "public"."notification" for all to "svetra_hole_notify_paid" using (true) with check (true);
create function "svetra_holes"."notify_paid"(p_caller uuid, p_old "public"."order", p_new "public"."order")
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'notify_paid is not written yet: edit supabase/holes/notify_paid.sql';
end
$svetra$;
alter function "svetra_holes"."notify_paid"(uuid, "public"."order", "public"."order") owner to "svetra_hole_notify_paid";
revoke all on function "svetra_holes"."notify_paid"(uuid, "public"."order", "public"."order") from public, anon, authenticated;
grant execute on function "svetra_holes"."notify_paid"(uuid, "public"."order", "public"."order") to current_user;
do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on function "svetra_holes"."notify_paid"(uuid, "public"."order", "public"."order") from service_role; end if; end $$;
create function "svetra_holes"."notify_paid__fire"()
  returns trigger language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if pg_trigger_depth() > 3 then
    raise exception 'notify_paid is part of a trigger loop' using errcode = '54001';
  end if;
  perform "svetra_holes"."notify_paid"((select auth.uid()),
    case when tg_op = 'INSERT' then null else old end,
    case when tg_op = 'DELETE' then null else new end);
  return null;
end
$svetra$;
revoke all on function "svetra_holes"."notify_paid__fire"() from public, anon, authenticated;
do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on function "svetra_holes"."notify_paid__fire"() from service_role; end if; end $$;
create trigger "svetra_notify_paid_update" after update on "public"."order"
  for each row when (new."status" = 'paid' and old."status" <> 'paid')
  execute function "svetra_holes"."notify_paid__fire"();

do $$ begin create role "svetra_hole_note_order_change" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_note_order_change" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_note_order_change";
grant usage on schema "public" to "svetra_hole_note_order_change";
grant select on table "public"."order" to "svetra_hole_note_order_change";
create policy "svetra_hole_note_order_change" on "public"."order" for select to "svetra_hole_note_order_change" using (true);
grant select, insert, update, delete on table "public"."notification" to "svetra_hole_note_order_change";
create policy "svetra_hole_note_order_change" on "public"."notification" for all to "svetra_hole_note_order_change" using (true) with check (true);
create function "svetra_holes"."note_order_change"(p_caller uuid, p_old "public"."order", p_new "public"."order")
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'note_order_change is not written yet: edit supabase/holes/note_order_change.sql';
end
$svetra$;
alter function "svetra_holes"."note_order_change"(uuid, "public"."order", "public"."order") owner to "svetra_hole_note_order_change";
revoke all on function "svetra_holes"."note_order_change"(uuid, "public"."order", "public"."order") from public, anon, authenticated;
grant execute on function "svetra_holes"."note_order_change"(uuid, "public"."order", "public"."order") to current_user;
do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on function "svetra_holes"."note_order_change"(uuid, "public"."order", "public"."order") from service_role; end if; end $$;
create function "svetra_holes"."note_order_change__fire"()
  returns trigger language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if pg_trigger_depth() > 3 then
    raise exception 'note_order_change is part of a trigger loop' using errcode = '54001';
  end if;
  perform "svetra_holes"."note_order_change"((select auth.uid()),
    case when tg_op = 'INSERT' then null else old end,
    case when tg_op = 'DELETE' then null else new end);
  return null;
end
$svetra$;
revoke all on function "svetra_holes"."note_order_change__fire"() from public, anon, authenticated;
do $$ begin if exists (select from pg_roles where rolname = 'service_role') then revoke all on function "svetra_holes"."note_order_change__fire"() from service_role; end if; end $$;
create trigger "svetra_note_order_change_insert" after insert on "public"."order"
  for each row when (new."total" > 0)
  execute function "svetra_holes"."note_order_change__fire"();
create trigger "svetra_note_order_change_delete" after delete on "public"."order"
  for each row when (old."total" > 0)
  execute function "svetra_holes"."note_order_change__fire"();
