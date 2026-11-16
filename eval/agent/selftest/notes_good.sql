create table public.note (id uuid primary key default gen_random_uuid(), owner uuid not null references auth.users (id), body text not null);
create index on public.note (owner);
alter table public.note enable row level security;
create policy own_notes on public.note for all to authenticated using ((select auth.uid()) = owner) with check ((select auth.uid()) = owner);
