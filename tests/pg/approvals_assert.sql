-- Behaviour of the compiled approvals app: a document moves draft -> submitted -> approved or
-- rejected by the right person, and nobody can skip a step. Every check raises when it fails.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000'), ('b0000000-0000-0000-0000-000000000000'), ('c0000000-0000-0000-0000-000000000000');

-- Alice (author) writes a draft naming bob as reviewer.
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
insert into public.doc (id, author, reviewer, title) values ('d1000000-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000000', 'first');
do $$ begin
  begin insert into public.doc (author, reviewer, title, status) values ('a0000000-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000000', 'x', 'approved'); raise exception 'alice created an approved document';
  exception when insufficient_privilege then null; end;
  begin insert into public.doc (author, reviewer, title) values ('a0000000-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 'x'); raise exception 'a document with the author as its own reviewer was accepted';
  exception when check_violation then null; end;
  -- A draft can be retitled, but not approved by its author.
  update public.doc set title = 'renamed';
  if (select title from public.doc) <> 'renamed' then raise exception 'alice could not retitle her draft'; end if;
  begin update public.doc set status = 'approved'; raise exception 'alice approved her own draft';
  exception when insufficient_privilege then null; end;
end $$;
-- Bob can't see a draft.
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin if (select count(*) from public.doc) <> 0 then raise exception 'the reviewer saw a draft'; end if; end $$;
-- Alice submits it.
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
update public.doc set status = 'submitted';
do $$ begin
  if (select status from public.doc) <> 'submitted' then raise exception 'alice could not submit her draft'; end if;
  -- After submitting she can no longer change it, delete it or approve it.
  update public.doc set title = 'too late';
  if (select title from public.doc) <> 'renamed' then raise exception 'alice changed a submitted document'; end if;
  -- No rule covers an update of a submitted document by its author: it finds no row, with no error.
  update public.doc set status = 'approved';
  if (select status from public.doc) <> 'submitted' then raise exception 'alice approved her own document'; end if;
  delete from public.doc;
  if (select count(*) from public.doc) <> 1 then raise exception 'alice deleted a submitted document'; end if;
end $$;
-- Bob, the reviewer, now sees it, and can approve it, but not skip back or change who reviews.
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.doc) <> 1 then raise exception 'the reviewer could not see a submitted document'; end if;
  begin update public.doc set status = 'draft'; raise exception 'the reviewer sent it back to draft';
  exception when insufficient_privilege then null; end;
  begin update public.doc set reviewer = 'c0000000-0000-0000-0000-000000000000'; raise exception 'the reviewer reassigned the document';
  exception when insufficient_privilege then null; end;
end $$;
update public.doc set status = 'approved';
do $$ begin
  if (select status from public.doc) <> 'approved' then raise exception 'the reviewer could not approve'; end if;
  -- Nobody can change an approved document.
  update public.doc set status = 'rejected';
  if (select status from public.doc) <> 'approved' then raise exception 'an approved document was changed'; end if;
end $$;
-- Carol sees nothing.
set request.jwt.claim.sub = 'c0000000-0000-0000-0000-000000000000';
do $$ begin if (select count(*) from public.doc) <> 0 then raise exception 'carol saw a document'; end if; end $$;
reset role;
set role anon;
do $$ begin if (select count(*) from public.doc) <> 0 then raise exception 'a visitor saw a document'; end if; end $$;
reset role;
