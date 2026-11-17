-- Journal backend: private entries with soft delete.

create table public.entry (
  id         uuid primary key default gen_random_uuid(),
  owner      uuid not null default auth.uid() references auth.users (id) on delete restrict,
  body       text not null,
  deleted_at timestamptz
);

create index entry_owner_idx on public.entry (owner);

alter table public.entry enable row level security;
alter table public.entry force row level security;

-- Start from nothing: Supabase's default privileges hand out everything.
revoke all on public.entry from public, anon, authenticated;

-- The app can read, create and edit entries. Column-level grants mean the app
-- can never write id or deleted_at, and can never change an entry's owner.
-- There is no DELETE privilege and no DELETE policy: nothing is removed.
grant select on public.entry to authenticated;
grant insert (owner, body) on public.entry to authenticated;
grant update (body) on public.entry to authenticated;

-- Soft-deleted rows are invisible to everyone through the app, owner included.
create policy entry_select_own on public.entry
  for select to authenticated
  using (owner = (select auth.uid()) and deleted_at is null);

create policy entry_insert_own on public.entry
  for insert to authenticated
  with check (owner = (select auth.uid()) and deleted_at is null);

-- Visible rows only (so a deleted entry can't be edited or revived), and the
-- result must stay the caller's own, not-deleted entry.
create policy entry_update_own on public.entry
  for update to authenticated
  using (owner = (select auth.uid()) and deleted_at is null)
  with check (owner = (select auth.uid()) and deleted_at is null);

-- The only way to delete. SECURITY DEFINER because the update policy above
-- deliberately forbids setting deleted_at; the function checks ownership itself.
create function public.soft_delete(entry_id uuid)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller uuid := auth.uid();
begin
  if caller is null then
    raise exception 'not authenticated' using errcode = '28000';
  end if;

  update public.entry
     set deleted_at = now()
   where id = entry_id
     and owner = caller
     and deleted_at is null;

  if not found then
    raise exception 'entry not found or not yours' using errcode = '42501';
  end if;
end;
$$;

revoke all on function public.soft_delete(uuid) from public, anon, authenticated;
grant execute on function public.soft_delete(uuid) to authenticated;
