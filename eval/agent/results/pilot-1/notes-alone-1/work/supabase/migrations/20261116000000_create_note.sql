-- Private notes: each signed-in person sees and changes only their own notes.

create table public.note (
  id    uuid primary key default gen_random_uuid(),
  owner uuid not null default auth.uid() references auth.users (id) on delete cascade,
  body  text not null
);

create index note_owner_idx on public.note (owner);

-- Row level security: enforced for every API role, including the table owner.
alter table public.note enable row level security;
alter table public.note force row level security;

-- Least privilege: signed-out (anon) callers get nothing; signed-in callers get
-- row access, and may not reassign ownership (only body is updatable).
revoke all on public.note from anon, authenticated, public;
grant select, insert, delete on public.note to authenticated;
grant update (body) on public.note to authenticated;

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
