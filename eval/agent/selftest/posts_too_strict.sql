create table public.post (id uuid primary key default gen_random_uuid(), author uuid not null references auth.users (id), title text not null, is_public boolean not null default false);
alter table public.post enable row level security;
create policy own_all on public.post for all to authenticated using ((select auth.uid()) = author) with check ((select auth.uid()) = author);
