-- Journal backend: private entries with soft delete.

create table public.entry (
  id         uuid primary key default gen_random_uuid(),
  owner      uuid not null references auth.users (id) on delete restrict,
  body       text not null,
  deleted_at timestamptz
);

create index entry_owner_idx on public.entry (owner);

alter table public.entry enable row level security;
alter table public.entry force row level security;

-- Privileges: the app roles get only what they need. No DELETE at all, and
-- no way to write deleted_at or change owner directly.
revoke all on public.entry from public, anon, authenticated;
grant select on public.entry to authenticated;
grant insert (owner, body) on public.entry to authenticated;
grant update (body) on public.entry to authenticated;

-- Owners see only their own entries that are not deleted (deleted ones are
-- hidden from everyone, including the owner).
create policy entry_select on public.entry
  for select to authenticated
  using (owner = (select auth.uid()) and deleted_at is null);

create policy entry_insert on public.entry
  for insert to authenticated
  with check (owner = (select auth.uid()) and deleted_at is null);

-- Edit only your own live entries; the row must stay yours and live.
create policy entry_update on public.entry
  for update to authenticated
  using (owner = (select auth.uid()) and deleted_at is null)
  with check (owner = (select auth.uid()) and deleted_at is null);

-- No delete policy: permanent removal is impossible through the app.
-- No policy can set deleted_at back to null: undelete is impossible too.

create function public.soft_delete(entry_id uuid)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  caller uuid := auth.uid();
  n integer;
begin
  if caller is null then
    raise exception 'not authenticated' using errcode = '42501';
  end if;

  update public.entry
     set deleted_at = now()
   where id = entry_id
     and owner = caller
     and deleted_at is null;

  get diagnostics n = row_count;
  if n = 0 then
    raise exception 'entry not found or not yours' using errcode = '42501';
  end if;
end;
$$;

revoke all on function public.soft_delete(uuid) from public, anon;
grant execute on function public.soft_delete(uuid) to authenticated;
