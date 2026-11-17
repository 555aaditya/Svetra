-- The mistake: the reviewer can approve and change the title in the same update.
create table public.doc (
  id uuid primary key default gen_random_uuid(),
  author uuid not null references auth.users (id),
  reviewer uuid not null references auth.users (id),
  title text not null,
  status text not null default 'draft' check (status in ('draft', 'submitted', 'approved', 'rejected'))
);
alter table public.doc enable row level security;
create policy see_docs on public.doc for select to authenticated
  using (author = (select auth.uid()) or (reviewer = (select auth.uid()) and status <> 'draft'));
create policy create_drafts on public.doc for insert to authenticated
  with check (author = (select auth.uid()) and status = 'draft');
create policy change_docs on public.doc for update to authenticated
  using ((author = (select auth.uid()) and status = 'draft') or (reviewer = (select auth.uid()) and status = 'submitted'))
  with check ((author = (select auth.uid()) and status in ('draft', 'submitted')) or (reviewer = (select auth.uid()) and status in ('approved', 'rejected')));
create policy delete_drafts on public.doc for delete to authenticated
  using (author = (select auth.uid()) and status = 'draft');
