-- The mistake: no way to delete at all, because an update that hides the row fails the select policy.
create table public.entry (id uuid primary key default gen_random_uuid(), owner uuid not null references auth.users (id), body text not null, deleted_at timestamptz);
alter table public.entry enable row level security;
create policy see_live on public.entry for select to authenticated using ((select auth.uid()) = owner and deleted_at is null);
create policy create_live on public.entry for insert to authenticated with check ((select auth.uid()) = owner and deleted_at is null);
create policy edit_live on public.entry for update to authenticated using ((select auth.uid()) = owner and deleted_at is null) with check ((select auth.uid()) = owner and deleted_at is null);
