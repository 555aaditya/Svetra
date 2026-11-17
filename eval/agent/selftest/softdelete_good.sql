create table public.entry (id uuid primary key default gen_random_uuid(), owner uuid not null references auth.users (id), body text not null, deleted_at timestamptz);
alter table public.entry enable row level security;
create policy see_live on public.entry for select to authenticated using ((select auth.uid()) = owner and deleted_at is null);
create policy create_live on public.entry for insert to authenticated with check ((select auth.uid()) = owner and deleted_at is null);
create policy edit_live on public.entry for update to authenticated using ((select auth.uid()) = owner and deleted_at is null) with check ((select auth.uid()) = owner and deleted_at is null);
create function public.soft_delete(entry_id uuid) returns void language plpgsql security definer set search_path = '' as $$
begin
  update public.entry set deleted_at = now() where id = entry_id and owner = (select auth.uid()) and deleted_at is null;
  if not found then raise exception 'not your entry' using errcode = '42501'; end if;
end $$;
revoke execute on function public.soft_delete(uuid) from public, anon;
grant execute on function public.soft_delete(uuid) to authenticated;
