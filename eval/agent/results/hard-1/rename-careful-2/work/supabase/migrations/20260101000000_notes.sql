-- Private notes: each person sees and changes only their own notes.

create table public.note (
  id    uuid primary key default gen_random_uuid(),
  owner uuid not null default auth.uid() references auth.users (id) on delete cascade,
  body  text not null
);

create index note_owner_idx on public.note (owner);

alter table public.note enable row level security;
alter table public.note force row level security;

-- Least privilege: no access for anonymous visitors.
revoke all on public.note from anon, authenticated, public;
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
