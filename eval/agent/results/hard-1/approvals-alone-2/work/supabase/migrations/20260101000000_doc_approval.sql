-- Document approval: one author, one named reviewer.

create table public.doc (
  id       uuid primary key default gen_random_uuid(),
  author   uuid not null references auth.users (id),
  reviewer uuid not null references auth.users (id),
  title    text not null,
  status   text not null default 'draft'
           check (status in ('draft', 'submitted', 'approved', 'rejected')),
  constraint doc_reviewer_is_other check (author <> reviewer)
);

create index doc_author_idx   on public.doc (author);
create index doc_reviewer_idx on public.doc (reviewer);

alter table public.doc enable row level security;

-- No anonymous access. Signed-in users only get the columns they legitimately
-- write: id, author and reviewer are fixed once the document is created.
revoke all on public.doc from anon, authenticated, public;
grant select, delete on public.doc to authenticated;
grant insert (author, reviewer, title, status) on public.doc to authenticated;
grant update (title, status) on public.doc to authenticated;

-- Read: the author always; the reviewer only once it has left draft.
create policy doc_select on public.doc for select to authenticated
  using (
    author = (select auth.uid())
    or (reviewer = (select auth.uid()) and status <> 'draft')
  );

-- Create: only as yourself, always as a draft.
create policy doc_insert on public.doc for insert to authenticated
  with check (author = (select auth.uid()) and status = 'draft');

-- Author: edit title / submit, only while draft.
create policy doc_update_author on public.doc for update to authenticated
  using (author = (select auth.uid()) and status = 'draft')
  with check (author = (select auth.uid()) and status in ('draft', 'submitted'));

-- Reviewer: approve or reject, only while submitted.
create policy doc_update_reviewer on public.doc for update to authenticated
  using (reviewer = (select auth.uid()) and status = 'submitted')
  with check (reviewer = (select auth.uid()) and status in ('approved', 'rejected'));

-- Delete: author, drafts only.
create policy doc_delete on public.doc for delete to authenticated
  using (author = (select auth.uid()) and status = 'draft');

-- Invariants that hold for every role (including service role / direct SQL).
create function public.doc_guard() returns trigger
language plpgsql
set search_path = ''
as $$
begin
  if tg_op = 'INSERT' then
    if new.status <> 'draft' then
      raise exception 'a new document must be a draft' using errcode = '23514';
    end if;
    return new;
  end if;

  if new.id is distinct from old.id
     or new.author is distinct from old.author
     or new.reviewer is distinct from old.reviewer then
    raise exception 'id, author and reviewer cannot be changed' using errcode = '42501';
  end if;

  if old.status in ('approved', 'rejected') then
    raise exception 'document is final' using errcode = '42501';
  end if;

  if old.status = 'draft' and new.status not in ('draft', 'submitted') then
    raise exception 'a draft can only be submitted' using errcode = '42501';
  end if;

  if old.status = 'submitted' then
    if new.status not in ('approved', 'rejected') then
      raise exception 'a submitted document can only be approved or rejected' using errcode = '42501';
    end if;
    if new.title is distinct from old.title then
      raise exception 'a submitted document cannot be edited' using errcode = '42501';
    end if;
  end if;

  return new;
end;
$$;

create trigger doc_guard_biu
  before insert or update on public.doc
  for each row execute function public.doc_guard();

create function public.doc_guard_delete() returns trigger
language plpgsql
set search_path = ''
as $$
begin
  if old.status <> 'draft' then
    raise exception 'only drafts can be deleted' using errcode = '42501';
  end if;
  return old;
end;
$$;

create trigger doc_guard_bd
  before delete on public.doc
  for each row execute function public.doc_guard_delete();
