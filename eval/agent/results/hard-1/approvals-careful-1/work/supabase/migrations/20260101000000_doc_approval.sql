-- Document approval: authors write drafts, one named reviewer approves/rejects.

create table public.doc (
  id uuid primary key default gen_random_uuid(),
  author uuid not null references auth.users (id),
  reviewer uuid not null references auth.users (id),
  title text not null,
  status text not null default 'draft'
    check (status in ('draft', 'submitted', 'approved', 'rejected')),
  constraint doc_reviewer_not_author check (author <> reviewer)
);

create index doc_author_idx on public.doc (author);
create index doc_reviewer_idx on public.doc (reviewer);

alter table public.doc enable row level security;
alter table public.doc force row level security;

-- No access for anonymous users; signed-in users get table access, narrowed by RLS and the trigger.
revoke all on public.doc from anon, public;
grant select, insert, update, delete on public.doc to authenticated;

-- Read: the author always; the reviewer only once submitted (never drafts).
create policy doc_select on public.doc for select to authenticated
  using (
    author = (select auth.uid())
    or (reviewer = (select auth.uid()) and status <> 'draft')
  );

-- Create: only as yourself, always as a draft, reviewer is someone else.
create policy doc_insert on public.doc for insert to authenticated
  with check (
    author = (select auth.uid())
    and status = 'draft'
    and reviewer <> author
  );

-- Author: may edit/submit only while draft; the result must be draft or submitted.
create policy doc_update_author on public.doc for update to authenticated
  using (author = (select auth.uid()) and status = 'draft')
  with check (author = (select auth.uid()) and status in ('draft', 'submitted'));

-- Reviewer: may only decide a submitted document.
create policy doc_update_reviewer on public.doc for update to authenticated
  using (reviewer = (select auth.uid()) and status = 'submitted')
  with check (reviewer = (select auth.uid()) and status in ('approved', 'rejected'));

-- Delete: author, drafts only.
create policy doc_delete on public.doc for delete to authenticated
  using (author = (select auth.uid()) and status = 'draft');

-- Column-level and state-machine rules, which RLS cannot express.
create or replace function public.doc_guard_update()
returns trigger
language plpgsql
set search_path = ''
as $$
begin
  if new.id is distinct from old.id
     or new.author is distinct from old.author
     or new.reviewer is distinct from old.reviewer then
    raise exception 'id, author and reviewer cannot be changed' using errcode = '42501';
  end if;

  if old.status in ('approved', 'rejected') then
    raise exception 'a % document cannot be changed', old.status using errcode = '42501';
  end if;

  -- Title can only change while the document is a draft.
  if new.title is distinct from old.title and old.status <> 'draft' then
    raise exception 'title can only be changed while the document is a draft' using errcode = '42501';
  end if;

  if not (
    (old.status = 'draft' and new.status in ('draft', 'submitted'))
    or (old.status = 'submitted' and new.status in ('submitted', 'approved', 'rejected'))
  ) then
    raise exception 'invalid status change from % to %', old.status, new.status using errcode = '42501';
  end if;

  return new;
end;
$$;

create trigger doc_guard_update
  before update on public.doc
  for each row execute function public.doc_guard_update();

revoke all on function public.doc_guard_update() from public, anon, authenticated;
