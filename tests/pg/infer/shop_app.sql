-- A small shop, as an AI app builder might write it, for `svetra infer` (docs/infer.md).
-- Applied after src/standin/base.sql. Covers each mapping, and what can't be translated.
create type public.order_status as enum ('open', 'paid', 'shipped');
create table public.profiles (
  id uuid primary key references auth.users (id),
  display_name text not null,
  bio varchar(280),
  created_at timestamptz not null default now()
);
create table public.products (
  id uuid primary key default gen_random_uuid(),
  sku text not null unique,
  name text not null,
  price numeric(12, 2) not null check (price >= 0),
  weight real,
  stock integer not null default 0,
  rating numeric,
  tags text[],
  meta jsonb,
  constraint stock_non_negative check (stock >= 0),
  constraint stock_text check (stock::text <> '-1')
);
create index products_tags_idx on public.products using gin (tags);
create table public.orders (
  id uuid primary key default gen_random_uuid(),
  customer uuid not null references auth.users (id) on delete cascade,
  status public.order_status not null default 'open',
  placed_on date default current_date,
  note text default 'none',
  created_at timestamp without time zone default now(),
  shipped boolean not null default false,
  check (status <> 'shipped' or shipped)
);
create index orders_customer_idx on public.orders (customer);
create index orders_note_idx on public.orders (lower(note));
create table public.order_items (
  id bigint generated always as identity primary key,
  order_id uuid not null references public.orders (id),
  product_id uuid not null references public.products (id),
  qty smallint not null check (qty > 0 and qty < least(10, 100)),
  unique (order_id, product_id)
);
create table public.legacy_codes (code text, label text);
create table public.membership (
  org uuid not null,
  member uuid not null references auth.users (id),
  primary key (org, member)
);
create view public.product_names as select id, name from public.products;
alter table public.profiles enable row level security;
create policy profiles_own on public.profiles for select to authenticated using (id = auth.uid());
alter table public.products enable row level security;
create policy products_read on public.products for select using (true);
