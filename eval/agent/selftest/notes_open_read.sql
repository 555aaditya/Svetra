create table public.note (id uuid primary key default gen_random_uuid(), owner uuid not null references auth.users (id), body text not null);
alter table public.note enable row level security;
create policy read_all on public.note for select using (true);
create policy own_write on public.note for all to authenticated using ((select auth.uid()) = owner) with check ((select auth.uid()) = owner);
