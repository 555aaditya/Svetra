-- Private notes: each person sees and changes only their own notes.

create table public.note (
  id    uuid primary key default gen_random_uuid(),
  owner uuid not null default auth.uid() references auth.users (id) on delete cascade,
  body  text not null
);

create index note_owner_idx on public.note (owner);

alter table public.note enable row level security;
alter table public.note force row level security;

-- No access for anonymous visitors; signed-in users get table privileges,
-- narrowed to their own rows by the policies below.
revoke all on public.note from anon, public;
grant select, insert, update, delete on public.note to authenticated;

create policy note_select_own on public.note
  for select to authenticated
  using (owner = (select auth.uid()));

create policy note_insert_own on public.note
  for insert to authenticated
  with check (owner = (select auth.uid()));

create policy note_update_own on public.note
  for update to authenticated
  using (owner = (select auth.uid()))
  with check (owner = (select auth.uid()));

create policy note_delete_own on public.note
  for delete to authenticated
  using (owner = (select auth.uid()));

-- Ownership is fixed at creation: block reassigning a note to someone else.
create function public.note_lock_owner() returns trigger
language plpgsql
set search_path = ''
as $$
begin
  if new.owner is distinct from old.owner then
    raise exception 'note owner cannot be changed';
  end if;
  return new;
end;
$$;

create trigger note_lock_owner
  before update on public.note
  for each row execute function public.note_lock_owner();
