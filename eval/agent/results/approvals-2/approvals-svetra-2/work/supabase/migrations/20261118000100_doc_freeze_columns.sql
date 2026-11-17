-- Row-level security decides which rows a person may touch and what a row may
-- become, but it cannot compare a row with its old self. This trigger holds the
-- rest of the rules: who a document belongs to and who reviews it never change,
-- the title changes only while the document is a draft, a status only moves
-- forward (draft -> submitted -> approved/rejected), and a decided document is
-- frozen. Together with the policies, the reviewer can change nothing but the
-- status, from submitted to approved or rejected.

create function public.doc_guard_update() returns trigger
language plpgsql
set search_path = ''
as $$
begin
  if new.id is distinct from old.id
     or new.author is distinct from old.author
     or new.reviewer is distinct from old.reviewer then
    raise exception 'a document''s id, author and reviewer cannot change'
      using errcode = '42501';
  end if;

  if old.status in ('approved', 'rejected') then
    raise exception 'a decided document cannot change' using errcode = '42501';
  end if;

  if new.title is distinct from old.title and old.status <> 'draft' then
    raise exception 'only a draft''s title can change' using errcode = '42501';
  end if;

  if new.status is distinct from old.status
     and not ((old.status = 'draft' and new.status = 'submitted')
           or (old.status = 'submitted' and new.status in ('approved', 'rejected'))) then
    raise exception 'invalid status change from % to %', old.status, new.status
      using errcode = '42501';
  end if;

  return new;
end;
$$;

create trigger doc_guard_update
  before update on public.doc
  for each row execute function public.doc_guard_update();
