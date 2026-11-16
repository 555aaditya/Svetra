create table public.post (id uuid primary key default gen_random_uuid(), author uuid not null references auth.users (id), title text not null, is_public boolean not null default false);
alter table public.post enable row level security;
create policy public_read on public.post for select to anon, authenticated using (is_public);
create policy own_read on public.post for select to authenticated using ((select auth.uid()) = author);
create policy own_write on public.post for insert to authenticated with check ((select auth.uid()) = author);
create policy own_update on public.post for update to authenticated using ((select auth.uid()) = author) with check ((select auth.uid()) = author);
create policy own_delete on public.post for delete to authenticated using ((select auth.uid()) = author);
