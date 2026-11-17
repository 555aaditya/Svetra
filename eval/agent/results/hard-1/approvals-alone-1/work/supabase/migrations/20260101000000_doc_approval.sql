-- Document approval: author writes, one named reviewer approves/rejects.

create table public.doc (
  id       uuid primary key default gen_random_uuid(),
  author   uuid not null references auth.users (id) on delete cascade,
  reviewer uuid not null references auth.users (id) on delete cascade,
  title    text not null,
  status   text not null default 'draft'
           check (status in ('draft', 'submitted', 'approved', 'rejected')),
  constraint doc_reviewer_is_other check (author <> reviewer)
);

create index doc_author_idx on public.doc (author);
create index doc_reviewer_idx on public.doc (reviewer);

alter table public.doc enable row level security;
alter table public.doc force row level security;

revoke all on public.doc from anon, authenticated;
grant select, insert, delete on public.doc to authenticated;
grant update (title, status) on public.doc to authenticated;

-- Visibility: author always; reviewer only once submitted (or decided).
create policy doc_select on public.doc for select to authenticated
  using (
    author = (select auth.uid())
    or (reviewer = (select auth.uid()) and status <> 'draft')
  );

-- Only as yourself, and always as a draft.
create policy doc_insert on public.doc for insert to authenticated
  with check (author = (select auth.uid()) and status = 'draft');

-- Author edits/submits drafts; reviewer decides submitted docs.
-- Which fields and transitions are allowed is enforced by the trigger below.
create policy doc_update on public.doc for update to authenticated
  using (
    (author = (select auth.uid()) and status = 'draft')
    or (reviewer = (select auth.uid()) and status = 'submitted')
  )
  with check (
    (author = (select auth.uid()) and status in ('draft', 'submitted'))
    or (reviewer = (select auth.uid()) and status in ('approved', 'rejected'))
  );

-- Only the author, only drafts.
create policy doc_delete on public.doc for delete to authenticated
  using (author = (select auth.uid()) and status = 'draft');

-- Field- and transition-level rules.
create function public.doc_guard_update() returns trigger
language plpgsql
set search_path = ''
as $$
declare
  uid uuid := auth.uid();
begin
  if new.id is distinct from old.id
     or new.author is distinct from old.author
     or new.reviewer is distinct from old.reviewer then
    raise exception 'id, author and reviewer cannot be changed'
      using errcode = '42501';
  end if;

  if old.status in ('approved', 'rejected') then
    raise exception 'a decided document cannot be changed'
      using errcode = '42501';
  end if;

  if old.status = 'draft' then
    if uid is distinct from old.author then
      raise exception 'only the author can change a draft' using errcode = '42501';
    end if;
    if new.status not in ('draft', 'submitted') then
      raise exception 'a draft can only be submitted' using errcode = '42501';
    end if;
  elsif old.status = 'submitted' then
    if uid is distinct from old.reviewer then
      raise exception 'only the reviewer can act on a submitted document'
        using errcode = '42501';
    end if;
    if new.title is distinct from old.title then
      raise exception 'the reviewer cannot change the title' using errcode = '42501';
    end if;
    if new.status not in ('approved', 'rejected') then
      raise exception 'a submitted document can only be approved or rejected'
        using errcode = '42501';
    end if;
  end if;

  return new;
end;
$$;

create trigger doc_guard_update
  before update on public.doc
  for each row execute function public.doc_guard_update();
