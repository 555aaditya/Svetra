-- Tables
create table "public"."wallet" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "balance" numeric(18, 2) not null,
  "label" text
);

-- Foreign keys
alter table "public"."wallet" add constraint "wallet_owner_fkey" foreign key ("owner") references "auth"."users" ("id");

-- Indexes
create index "wallet_owner_idx" on "public"."wallet" ("owner");

-- Access
alter table "public"."wallet" enable row level security;

-- Policies
create policy "owner_read" on "public"."wallet"
  for select to authenticated
  using ("owner" = (select auth.uid()));

-- Holes
create schema if not exists "svetra_holes";

do $$ begin create role "svetra_hole_deposit" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_deposit" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_deposit";
grant usage on schema "public" to "svetra_hole_deposit";
grant select, insert, update, delete on table "public"."wallet" to "svetra_hole_deposit";
create policy "svetra_hole_deposit" on "public"."wallet" for all to "svetra_hole_deposit" using (true) with check (true);
create function "svetra_holes"."deposit"(p_caller uuid, p_wallet uuid, p_amount numeric(18, 2))
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce((select t."owner" from "public"."wallet" t where t."id" = p_wallet) = p_caller, false) then
    raise exception 'not allowed to call deposit' using errcode = '42501';
  end if;
  if not coalesce(p_amount > 0, false) then
    raise exception using message = 'deposit: amount > 0', errcode = '22023';
  end if;
  if not coalesce(p_amount <= 10000, false) then
    raise exception using message = 'deposit: amount <= 10000', errcode = '22023';
  end if;
  raise exception 'deposit is not written yet: edit supabase/holes/deposit.sql';
end
$svetra$;
alter function "svetra_holes"."deposit"(uuid, uuid, numeric(18, 2)) owner to "svetra_hole_deposit";
revoke all on function "svetra_holes"."deposit"(uuid, uuid, numeric(18, 2)) from public, anon, authenticated;
grant execute on function "svetra_holes"."deposit"(uuid, uuid, numeric(18, 2)) to current_user;
create function "public"."deposit"(wallet uuid, amount numeric(18, 2))
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin perform "svetra_holes"."deposit"((select auth.uid()), wallet, amount); end $svetra$;
revoke all on function "public"."deposit"(uuid, numeric(18, 2)) from public, anon, authenticated;
grant execute on function "public"."deposit"(uuid, numeric(18, 2)) to authenticated;

do $$ begin create role "svetra_hole_relabel" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_relabel" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_relabel";
grant usage on schema "public" to "svetra_hole_relabel";
grant select, insert, update, delete on table "public"."wallet" to "svetra_hole_relabel";
create policy "svetra_hole_relabel" on "public"."wallet" for all to "svetra_hole_relabel" using (true) with check (true);
create function "svetra_holes"."relabel"(p_caller uuid, p_wallet uuid, p_label text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce((select t."owner" from "public"."wallet" t where t."id" = p_wallet) = p_caller, false) then
    raise exception 'not allowed to call relabel' using errcode = '42501';
  end if;
  if not coalesce(p_label is not null and p_label <> '100%', false) then
    raise exception using message = 'relabel: label != null and label != ''100%''', errcode = '22023';
  end if;
  raise exception 'relabel is not written yet: edit supabase/holes/relabel.sql';
end
$svetra$;
alter function "svetra_holes"."relabel"(uuid, uuid, text) owner to "svetra_hole_relabel";
revoke all on function "svetra_holes"."relabel"(uuid, uuid, text) from public, anon, authenticated;
grant execute on function "svetra_holes"."relabel"(uuid, uuid, text) to current_user;
create function "public"."relabel"(wallet uuid, label text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin perform "svetra_holes"."relabel"((select auth.uid()), wallet, label); end $svetra$;
revoke all on function "public"."relabel"(uuid, text) from public, anon, authenticated;
grant execute on function "public"."relabel"(uuid, text) to authenticated;

do $$ begin create role "svetra_hole_move" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_move" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_move";
grant usage on schema "public" to "svetra_hole_move";
grant select on table "public"."wallet" to "svetra_hole_move";
create policy "svetra_hole_move" on "public"."wallet" for select to "svetra_hole_move" using (true);
create function "svetra_holes"."move"(p_caller uuid, p_source uuid, p_destination uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce((select t."owner" from "public"."wallet" t where t."id" = p_source) = p_caller, false) then
    raise exception 'not allowed to call move' using errcode = '42501';
  end if;
  if not coalesce(p_source <> p_destination, false) then
    raise exception using message = 'move: source != destination', errcode = '22023';
  end if;
  raise exception 'move is not written yet: edit supabase/holes/move.sql';
end
$svetra$;
alter function "svetra_holes"."move"(uuid, uuid, uuid) owner to "svetra_hole_move";
revoke all on function "svetra_holes"."move"(uuid, uuid, uuid) from public, anon, authenticated;
grant execute on function "svetra_holes"."move"(uuid, uuid, uuid) to current_user;
create function "public"."move"(source uuid, destination uuid)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$ begin perform "svetra_holes"."move"((select auth.uid()), source, destination); end $svetra$;
revoke all on function "public"."move"(uuid, uuid) from public, anon, authenticated;
grant execute on function "public"."move"(uuid, uuid) to authenticated;
