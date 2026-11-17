-- Journal entries: private per owner, soft-deleted only.

create table public.entry (
  id         uuid primary key default gen_random_uuid(),
  owner      uuid not null references auth.users (id) default auth.uid(),
  body       text not null,
  deleted_at timestamptz
);

create index entry_owner_idx on public.entry (owner);

alter table public.entry enable row level security;

-- Privileges: the app roles can never delete rows, and can only write the
-- columns they legitimately control. deleted_at can only be set through
-- soft_delete(); owner can never change after creation.
revoke all on public.entry from public, anon, authenticated;
grant select on public.entry to authenticated;
grant insert (owner, body) on public.entry to authenticated;
grant update (body) on public.entry to authenticated;

-- Owners see only their own live entries (deleted ones are hidden from everyone).
create policy entry_select on public.entry
  for select to authenticated
  using (owner = (select auth.uid()) and deleted_at is null);

create policy entry_insert on public.entry
  for insert to authenticated
  with check (owner = (select auth.uid()) and deleted_at is null);

create policy entry_update on public.entry
  for update to authenticated
  using (owner = (select auth.uid()) and deleted_at is null)
  with check (owner = (select auth.uid()) and deleted_at is null);

-- No delete policy and no DELETE privilege: hard deletes are impossible via the app.

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
    raise exception 'entry not found or not owned by caller' using errcode = '42501';
  end if;
end;
$$;

revoke all on function public.soft_delete(uuid) from public, anon;
grant execute on function public.soft_delete(uuid) to authenticated;
