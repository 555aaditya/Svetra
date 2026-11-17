-- Journal: private entries with soft delete.

create table public.entry (
  id         uuid primary key default gen_random_uuid(),
  owner      uuid not null default auth.uid() references auth.users (id) on delete restrict,
  body       text not null,
  deleted_at timestamptz
);

create index entry_owner_idx on public.entry (owner);

alter table public.entry enable row level security;
alter table public.entry force row level security;

-- Start from nothing, then grant only what the app needs.
revoke all on public.entry from public, anon, authenticated;

-- Column-level grants: callers may set owner/body on insert and change only
-- body on update. deleted_at and id can never be written directly, and there
-- is no DELETE privilege at all.
grant select on public.entry to authenticated;
grant insert (owner, body) on public.entry to authenticated;
grant update (body) on public.entry to authenticated;

-- Owners see only their own, non-deleted entries (deleted ones are hidden from everyone).
create policy entry_select_own on public.entry
  for select to authenticated
  using (owner = (select auth.uid()) and deleted_at is null);

create policy entry_insert_own on public.entry
  for insert to authenticated
  with check (owner = (select auth.uid()) and deleted_at is null);

-- Edit own live entries only; row must stay theirs and live.
create policy entry_update_own on public.entry
  for update to authenticated
  using (owner = (select auth.uid()) and deleted_at is null)
  with check (owner = (select auth.uid()) and deleted_at is null);

-- No DELETE policy: permanent removal is impossible through the app.

-- The only way to delete: mark the caller's own live entry as deleted.
create function public.soft_delete(entry_id uuid)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller uuid := auth.uid();
  affected int;
begin
  if caller is null then
    raise exception 'not authenticated' using errcode = '28000';
  end if;

  update public.entry
     set deleted_at = now()
   where id = entry_id
     and owner = caller
     and deleted_at is null;

  get diagnostics affected = row_count;
  if affected = 0 then
    raise exception 'entry not found or not yours' using errcode = '42501';
  end if;
end;
$$;

revoke all on function public.soft_delete(uuid) from public, anon;
grant execute on function public.soft_delete(uuid) to authenticated;
