-- Tables
create table "public"."wallet" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "balance" numeric(18, 2) not null,
  "label" text
);

create table "public"."payment" (
  "id" uuid primary key default gen_random_uuid(),
  "wallet" uuid not null,
  "amount" numeric(18, 2) not null
);

-- Foreign keys
alter table "public"."wallet" add constraint "wallet_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."payment" add constraint "payment_wallet_fkey" foreign key ("wallet") references "public"."wallet" ("id");

-- Indexes
create index "wallet_owner_idx" on "public"."wallet" ("owner");
create index "payment_wallet_idx" on "public"."payment" ("wallet");

-- Access
alter table "public"."wallet" enable row level security;
alter table "public"."payment" enable row level security;

-- Policies
create policy "owner_read" on "public"."wallet"
  for select to authenticated
  using ("owner" = (select auth.uid()));

create policy "payments_read" on "public"."payment"
  for select to authenticated
  using ((select t."owner" from "public"."wallet" t where t."id" = "payment"."wallet") = (select auth.uid()));

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
create table "svetra"."hole_keys" (
  "hole" text not null,
  "caller" uuid not null,
  "key" text not null,
  "inputs" jsonb not null,
  "result" jsonb,
  primary key ("hole", "caller", "key")
);
alter table "svetra"."hole_keys" enable row level security;
revoke all on table "svetra"."hole_keys" from public, anon, authenticated;

do $$ begin create role "svetra_hole_pay" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_pay" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_pay";
grant usage on schema "public" to "svetra_hole_pay";
grant select, insert, update, delete on table "public"."wallet" to "svetra_hole_pay";
create policy "svetra_hole_pay" on "public"."wallet" for all to "svetra_hole_pay" using (true) with check (true);
grant select, insert, update, delete on table "public"."payment" to "svetra_hole_pay";
create policy "svetra_hole_pay" on "public"."payment" for all to "svetra_hole_pay" using (true) with check (true);
create function "svetra_holes"."pay_allowed"(p_caller uuid, p_request uuid, p_wallet uuid, p_amount numeric(18, 2))
  returns boolean language plpgsql security definer set search_path = ''
  as $svetra$
begin
  return coalesce((select t."owner" from "public"."wallet" t where t."id" = p_wallet) = p_caller, false);
end
$svetra$;
alter function "svetra_holes"."pay_allowed"(uuid, uuid, uuid, numeric(18, 2)) owner to "svetra_hole_pay";
revoke all on function "svetra_holes"."pay_allowed"(uuid, uuid, uuid, numeric(18, 2)) from public, anon, authenticated;
grant execute on function "svetra_holes"."pay_allowed"(uuid, uuid, uuid, numeric(18, 2)) to current_user;
create function "svetra_holes"."pay"(p_caller uuid, p_request uuid, p_wallet uuid, p_amount numeric(18, 2))
  returns "public"."payment" language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce("svetra_holes"."pay_allowed"(p_caller, p_request, p_wallet, p_amount), false) then
    raise exception 'not allowed to call pay' using errcode = '42501';
  end if;
  if not coalesce(p_amount > 0, false) then
    raise exception using message = 'pay: amount > 0', errcode = '22023';
  end if;
  raise exception 'pay is not written yet: edit supabase/holes/pay.sql';
end
$svetra$;
alter function "svetra_holes"."pay"(uuid, uuid, uuid, numeric(18, 2)) owner to "svetra_hole_pay";
revoke all on function "svetra_holes"."pay"(uuid, uuid, uuid, numeric(18, 2)) from public, anon, authenticated;
grant execute on function "svetra_holes"."pay"(uuid, uuid, uuid, numeric(18, 2)) to current_user;
create function "public"."pay"(request uuid, wallet uuid, amount numeric(18, 2))
  returns "public"."payment" language plpgsql security definer set search_path = ''
  as $svetra$
#variable_conflict use_column
declare
  v_key text := (request)::text;
  v_inputs jsonb := jsonb_build_object('request', request, 'wallet', wallet, 'amount', amount);
  v_row "svetra"."hole_keys";
  v_result "public"."payment";
begin
  if v_key is null then raise exception using message = 'pay: request is needed so a repeat can be recognised', errcode = '22023'; end if;
  select * into v_row from "svetra"."hole_keys" where "hole" = 'pay' and "caller" = (select auth.uid()) and "key" = v_key;
  if found then
    if not coalesce("svetra_holes"."pay_allowed"((select auth.uid()), request, wallet, amount), false) then raise exception 'not allowed to call pay' using errcode = '42501'; end if;
    if v_row."inputs" is distinct from v_inputs then raise exception using message = 'pay: request was already used with different inputs', errcode = '22023'; end if;
    return jsonb_populate_record(null::"public"."payment", v_row."result");
  end if;
   insert into "svetra"."audit_log" ("actor", "kind", "subject", "action", "new") values ((select auth.uid()), 'call', 'pay', 'call', jsonb_build_object('request', request, 'wallet', wallet, 'amount', amount)); v_result := "svetra_holes"."pay"((select auth.uid()), request, wallet, amount);
  insert into "svetra"."hole_keys" ("hole", "caller", "key", "inputs", "result") values ('pay', (select auth.uid()), v_key, v_inputs, to_jsonb(v_result)); return v_result;
end
$svetra$;
revoke all on function "public"."pay"(uuid, uuid, numeric(18, 2)) from public, anon, authenticated;
grant execute on function "public"."pay"(uuid, uuid, numeric(18, 2)) to authenticated;

do $$ begin create role "svetra_hole_relabel" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_relabel" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_relabel";
grant usage on schema "public" to "svetra_hole_relabel";
grant select, insert, update, delete on table "public"."wallet" to "svetra_hole_relabel";
create policy "svetra_hole_relabel" on "public"."wallet" for all to "svetra_hole_relabel" using (true) with check (true);
create function "svetra_holes"."relabel"(p_caller uuid, p_tag text, p_wallet uuid, p_label text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'relabel is not written yet: edit supabase/holes/relabel.sql';
end
$svetra$;
alter function "svetra_holes"."relabel"(uuid, text, uuid, text) owner to "svetra_hole_relabel";
revoke all on function "svetra_holes"."relabel"(uuid, text, uuid, text) from public, anon, authenticated;
grant execute on function "svetra_holes"."relabel"(uuid, text, uuid, text) to current_user;
create function "public"."relabel"(tag text, wallet uuid, label text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
#variable_conflict use_column
declare
  v_key text := (tag)::text;
  v_inputs jsonb := jsonb_build_object('tag', tag, 'wallet', wallet, 'label', label);
  v_row "svetra"."hole_keys";
begin
  if v_key is null then raise exception using message = 'relabel: tag is needed so a repeat can be recognised', errcode = '22023'; end if;
  select * into v_row from "svetra"."hole_keys" where "hole" = 'relabel' and "caller" = (select auth.uid()) and "key" = v_key;
  if found then
    if v_row."inputs" is distinct from v_inputs then raise exception using message = 'relabel: tag was already used with different inputs', errcode = '22023'; end if;
    return;
  end if;
   perform "svetra_holes"."relabel"((select auth.uid()), tag, wallet, label);
  insert into "svetra"."hole_keys" ("hole", "caller", "key", "inputs", "result") values ('relabel', (select auth.uid()), v_key, v_inputs, null::jsonb);
end
$svetra$;
revoke all on function "public"."relabel"(text, uuid, text) from public, anon, authenticated;
grant execute on function "public"."relabel"(text, uuid, text) to authenticated;

do $$ begin create role "svetra_hole_total" nologin; exception when duplicate_object then null; end $$;
grant "svetra_hole_total" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_total";
grant usage on schema "public" to "svetra_hole_total";
grant select on table "public"."wallet" to "svetra_hole_total";
create policy "svetra_hole_total" on "public"."wallet" for select to "svetra_hole_total" using (true);
create function "svetra_holes"."total"(p_caller uuid, p_request text, p_wallet uuid)
  returns numeric(18, 2) language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'total is not written yet: edit supabase/holes/total.sql';
end
$svetra$;
alter function "svetra_holes"."total"(uuid, text, uuid) owner to "svetra_hole_total";
revoke all on function "svetra_holes"."total"(uuid, text, uuid) from public, anon, authenticated;
grant execute on function "svetra_holes"."total"(uuid, text, uuid) to current_user;
create function "public"."total"(request text, wallet uuid)
  returns numeric(18, 2) language plpgsql security definer set search_path = ''
  as $svetra$
#variable_conflict use_column
declare
  v_key text := (request)::text;
  v_inputs jsonb := jsonb_build_object('request', request, 'wallet', wallet);
  v_row "svetra"."hole_keys";
  v_result numeric(18, 2);
begin
  if v_key is null then raise exception using message = 'total: request is needed so a repeat can be recognised', errcode = '22023'; end if;
  select * into v_row from "svetra"."hole_keys" where "hole" = 'total' and "caller" = (select auth.uid()) and "key" = v_key;
  if found then
    if v_row."inputs" is distinct from v_inputs then raise exception using message = 'total: request was already used with different inputs', errcode = '22023'; end if;
    return (v_row."result" #>> '{}')::numeric(18, 2);
  end if;
   v_result := "svetra_holes"."total"((select auth.uid()), request, wallet);
  insert into "svetra"."hole_keys" ("hole", "caller", "key", "inputs", "result") values ('total', (select auth.uid()), v_key, v_inputs, to_jsonb(v_result)); return v_result;
end
$svetra$;
revoke all on function "public"."total"(text, uuid) from public, anon, authenticated;
grant execute on function "public"."total"(text, uuid) to authenticated;
