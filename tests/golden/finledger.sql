-- Tables
create table "public"."account" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "currency" text not null,
  "balance" numeric(18, 6) not null,
  "status" text default 'active',
  "created_at" timestamptz default now(),
  constraint "account_currency_values" check ("currency" in ('USD', 'EUR', 'GBP', 'INR')),
  constraint "account_status_values" check ("status" in ('active', 'frozen', 'closed')),
  constraint "balance_non_negative" check ("balance" >= 0),
  constraint "account_owner_currency_key" unique ("owner", "currency")
);

create table "public"."transaction" (
  "id" uuid primary key default gen_random_uuid(),
  "source" uuid not null,
  "destination" uuid not null,
  "amount" numeric(18, 6) not null,
  "status" text,
  "created_at" timestamptz default now(),
  constraint "transaction_status_values" check ("status" in ('pending', 'committed', 'failed', 'reversed')),
  constraint "amount_positive" check ("amount" > 0),
  constraint "different_accounts" check ("source" <> "destination")
);

-- Foreign keys
alter table "public"."account" add constraint "account_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."transaction" add constraint "transaction_source_fkey" foreign key ("source") references "public"."account" ("id");
alter table "public"."transaction" add constraint "transaction_destination_fkey" foreign key ("destination") references "public"."account" ("id");

-- Indexes
create index "transaction_source_created_at_idx" on "public"."transaction" ("source", "created_at");
create index "transaction_destination_created_at_idx" on "public"."transaction" ("destination", "created_at");

-- Access
alter table "public"."account" enable row level security;
alter table "public"."transaction" enable row level security;

-- Policies
create policy "owner_reads_own_accounts" on "public"."account"
  for select to authenticated
  using ("owner" = (select auth.uid()));

create policy "owner_reads_own_transactions" on "public"."transaction"
  for select to authenticated
  using ((select t."owner" from "public"."account" t where t."id" = "transaction"."source") = (select auth.uid()) or (select t."owner" from "public"."account" t where t."id" = "transaction"."destination") = (select auth.uid()));

-- Holes
create schema if not exists "svetra_holes";

create role "svetra_hole_transfer" nologin;
grant "svetra_hole_transfer" to current_user;
grant usage, create on schema "svetra_holes" to "svetra_hole_transfer";
grant usage on schema "public" to "svetra_hole_transfer";
grant select, insert, update, delete on table "public"."account" to "svetra_hole_transfer";
create policy "svetra_hole_transfer" on "public"."account" for all to "svetra_hole_transfer" using (true) with check (true);
grant select, insert, update, delete on table "public"."transaction" to "svetra_hole_transfer";
create policy "svetra_hole_transfer" on "public"."transaction" for all to "svetra_hole_transfer" using (true) with check (true);
create function "svetra_holes"."transfer"(p_caller uuid, p_source uuid, p_destination uuid, p_amount numeric(18, 6))
  returns "public"."transaction" language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce((select t."owner" from "public"."account" t where t."id" = p_source) = p_caller, false) then
    raise exception 'not allowed to call transfer' using errcode = '42501';
  end if;
  raise exception 'transfer is not written yet: edit supabase/holes/transfer.sql';
end
$svetra$;
alter function "svetra_holes"."transfer"(uuid, uuid, uuid, numeric(18, 6)) owner to "svetra_hole_transfer";
revoke all on function "svetra_holes"."transfer"(uuid, uuid, uuid, numeric(18, 6)) from public;
grant execute on function "svetra_holes"."transfer"(uuid, uuid, uuid, numeric(18, 6)) to current_user;
create function "public"."transfer"(source uuid, destination uuid, amount numeric(18, 6))
  returns "public"."transaction" language plpgsql security definer set search_path = ''
  as $svetra$ begin return "svetra_holes"."transfer"((select auth.uid()), source, destination, amount); end $svetra$;
revoke all on function "public"."transfer"(uuid, uuid, numeric(18, 6)) from public;
grant execute on function "public"."transfer"(uuid, uuid, numeric(18, 6)) to authenticated;
