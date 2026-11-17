-- Document approval: author writes, one named reviewer approves or rejects.

create table public.doc (
  id       uuid primary key default gen_random_uuid(),
  author   uuid not null references auth.users (id) on delete cascade,
  reviewer uuid not null references auth.users (id) on delete cascade,
  title    text not null,
  status   text not null default 'draft'
           check (status in ('draft', 'submitted', 'approved', 'rejected')),
  -- the reviewer is "one other person": an author can never review their own document
  constraint doc_reviewer_is_not_author check (author <> reviewer)
);

create index doc_author_idx   on public.doc (author);
create index doc_reviewer_idx on public.doc (reviewer);

alter table public.doc enable row level security;

revoke all on public.doc from anon, authenticated;
grant select, insert, update, delete on public.doc to authenticated;

-- Read: author always; reviewer only once submitted (and afterwards).
create policy doc_select on public.doc for select to authenticated
  using (
    author = (select auth.uid())
    or (reviewer = (select auth.uid()) and status <> 'draft')
  );

-- Create: only as yourself, always as a draft.
create policy doc_insert on public.doc for insert to authenticated
  with check (
    author = (select auth.uid())
    and status = 'draft'
    and reviewer <> author
  );

-- Update: author on drafts (edit / submit); reviewer on submitted (decide).
-- Which columns may change is enforced by the trigger below.
create policy doc_update on public.doc for update to authenticated
  using (
    (author = (select auth.uid()) and status = 'draft')
    or (reviewer = (select auth.uid()) and status = 'submitted')
  )
  with check (
    (author = (select auth.uid()) and status in ('draft', 'submitted'))
    or (reviewer = (select auth.uid()) and status in ('approved', 'rejected'))
  );

-- Delete: author, drafts only.
create policy doc_delete on public.doc for delete to authenticated
  using (author = (select auth.uid()) and status = 'draft');

-- Column and transition rules RLS cannot express.
create function public.doc_guard_update() returns trigger
language plpgsql
set search_path = ''
as $$
begin
  -- Only constrain API roles; admin/migration roles manage data directly.
  if current_user not in ('anon', 'authenticated') then
    return new;
  end if;

  if new.id is distinct from old.id
     or new.author is distinct from old.author
     or new.reviewer is distinct from old.reviewer then
    raise exception 'id, author and reviewer cannot be changed' using errcode = '42501';
  end if;

  if old.status = 'draft' then
    if (select auth.uid()) is distinct from old.author
       or new.status not in ('draft', 'submitted') then
      raise exception 'invalid change to a draft' using errcode = '42501';
    end if;
  elsif old.status = 'submitted' then
    if (select auth.uid()) is distinct from old.reviewer
       or new.status not in ('approved', 'rejected')
       or new.title is distinct from old.title then
      raise exception 'a submitted document can only be approved or rejected by its reviewer'
        using errcode = '42501';
    end if;
  else
    raise exception 'a % document cannot be changed', old.status using errcode = '42501';
  end if;

  return new;
end;
$$;

create trigger doc_guard_update
  before update on public.doc
  for each row execute function public.doc_guard_update();
