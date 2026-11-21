-- Behaviour of the compiled forum example: what deleting a referenced row does for each on_delete.
-- Rows are read back after each delete, because a statement that "succeeds" may have touched nothing.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000');
insert into public.board (id, owner, name) values ('b0000000-0000-0000-0000-000000000001', 'a0000000-0000-0000-0000-000000000000', 'general');
insert into public.thread (id, owner, board, title) values ('b0000000-0000-0000-0000-000000000002', 'a0000000-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000001', 'hello');
insert into public.post (id, owner, thread, body) values ('b0000000-0000-0000-0000-000000000003', 'a0000000-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000002', 'first');
insert into public.post (id, owner, thread, reply_to, body) values ('b0000000-0000-0000-0000-000000000004', 'a0000000-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000002', 'b0000000-0000-0000-0000-000000000003', 'reply');
insert into public.reaction (id, owner, post) values ('b0000000-0000-0000-0000-000000000005', 'a0000000-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000003');
insert into public.tag (id, owner, post, label) values ('b0000000-0000-0000-0000-000000000006', 'a0000000-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000003', 'news');

-- restrict: a thread with posts can't be deleted, and neither can the board above it (the thread
-- cascades, then the posts block it). Nothing changes.
do $$ begin
  begin delete from public.thread where id = 'b0000000-0000-0000-0000-000000000002'; raise exception 'a thread with posts was deleted';
  exception when foreign_key_violation then null; end;
  begin delete from public.board where id = 'b0000000-0000-0000-0000-000000000001'; raise exception 'a board whose thread has posts was deleted';
  exception when foreign_key_violation then null; end;
  if (select count(*) from public.board) <> 1 or (select count(*) from public.thread) <> 1 then raise exception 'a refused delete changed rows'; end if;
end $$;

-- set_null and cascade: deleting the first post keeps the reply (its reply_to is cleared) and the
-- tag (its post is cleared), and removes the reaction.
delete from public.post where id = 'b0000000-0000-0000-0000-000000000003';
do $$ begin
  if (select count(*) from public.post) <> 1 then raise exception 'only the first post should be gone'; end if;
  if (select reply_to from public.post) is not null then raise exception 'the reply should have lost its reply_to'; end if;
  if (select count(*) from public.tag where post is null) <> 1 then raise exception 'the tag should stay, with no post'; end if;
  if (select count(*) from public.reaction) <> 0 then raise exception 'the reaction should have been deleted with its post'; end if;
end $$;

-- A thread with no posts left can be deleted, and the board cascade then removes the rest.
delete from public.post;
delete from public.board where id = 'b0000000-0000-0000-0000-000000000001';
do $$ begin
  if (select count(*) from public.board) <> 0 or (select count(*) from public.thread) <> 0 then raise exception 'deleting the board should delete its thread'; end if;
end $$;
