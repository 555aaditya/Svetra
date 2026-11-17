-- Document approval: an author writes a document and names one reviewer.

create table public.doc (
  id       uuid primary key default gen_random_uuid(),
  author   uuid not null references auth.users (id),
  reviewer uuid not null references auth.users (id),
  title    text not null,
  status   text not null default 'draft'
    constraint doc_status_check check (status in ('draft', 'submitted', 'approved', 'rejected')),
  constraint doc_reviewer_not_author check (reviewer <> author)
);

create index doc_author_idx   on public.doc (author);
create index doc_reviewer_idx on public.doc (reviewer);

alter table public.doc enable row level security;

-- Privileges: signed-in users only, and only the columns that may ever change.
revoke all on public.doc from anon, authenticated;
grant select, insert, delete on public.doc to authenticated;
grant update (title, status) on public.doc to authenticated;

-- Who can see: the author always; the reviewer once it is no longer a draft.
create policy doc_select on public.doc
  for select to authenticated
  using (
    author = (select auth.uid())
    or (reviewer = (select auth.uid()) and status <> 'draft')
  );

-- Who can create: anyone, as author, as a draft.
create policy doc_insert on public.doc
  for insert to authenticated
  with check (author = (select auth.uid()) and status = 'draft');

-- Who can change: the author while it is a draft (edit, or submit);
-- the reviewer while it is submitted (approve or reject).
create policy doc_update on public.doc
  for update to authenticated
  using (
    (author = (select auth.uid()) and status = 'draft')
    or (reviewer = (select auth.uid()) and status = 'submitted')
  )
  with check (
    (author = (select auth.uid()) and status in ('draft', 'submitted'))
    or (reviewer = (select auth.uid()) and status in ('approved', 'rejected'))
  );

-- Who can delete: the author, only while it is a draft.
create policy doc_delete on public.doc
  for delete to authenticated
  using (author = (select auth.uid()) and status = 'draft');

-- Rules a row policy cannot express: new rows are drafts, and which changes are allowed.
create function public.doc_guard()
returns trigger
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

  if old.status = 'draft' then
    if new.status not in ('draft', 'submitted') then
      raise exception 'a draft can only be submitted' using errcode = '42501';
    end if;
  elsif old.status = 'submitted' then
    if new.status not in ('approved', 'rejected') then
      raise exception 'a submitted document can only be approved or rejected' using errcode = '42501';
    end if;
    if new.title is distinct from old.title then
      raise exception 'a submitted document cannot be edited' using errcode = '42501';
    end if;
  else
    raise exception 'a % document cannot be changed', old.status using errcode = '42501';
  end if;

  return new;
end;
$$;

create trigger doc_guard_insert before insert on public.doc
  for each row execute function public.doc_guard();
create trigger doc_guard_update before update on public.doc
  for each row execute function public.doc_guard();

-- Only drafts can be deleted, for every role.
create function public.doc_guard_delete()
returns trigger
language plpgsql
set search_path = ''
as $$
begin
  if old.status <> 'draft' then
    raise exception 'only a draft can be deleted' using errcode = '42501';
  end if;
  return old;
end;
$$;

create trigger doc_guard_delete before delete on public.doc
  for each row execute function public.doc_guard_delete();
