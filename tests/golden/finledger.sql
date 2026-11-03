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
