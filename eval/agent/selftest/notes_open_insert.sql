create table public.note (id uuid primary key default gen_random_uuid(), owner uuid not null references auth.users (id), body text not null);
alter table public.note enable row level security;
create policy own_read on public.note for select to authenticated using ((select auth.uid()) = owner);
create policy any_insert on public.note for insert to authenticated with check (true);
create policy own_update on public.note for update to authenticated using ((select auth.uid()) = owner) with check ((select auth.uid()) = owner);
create policy own_delete on public.note for delete to authenticated using ((select auth.uid()) = owner);
