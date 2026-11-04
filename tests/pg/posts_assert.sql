-- Behaviour of the compiled posts example: a private table, a four-action author policy and a
-- public read policy. After every write, read back as each role: a write that "succeeds" under
-- row-level security may have changed nothing, so only a readback proves what happened.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000'), ('b0000000-0000-0000-0000-000000000000');
insert into public.post (id, author, body) values ('11111111-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 'hello');

-- Public read: everyone sees the post, signed in or not. Nobody reaches the private audit log.
set role anon;
do $$ begin
  if (select count(*) from public.post) <> 1 then raise exception 'logged-out visitors should read the public post'; end if;
  begin perform count(*) from public.audit_log; raise exception 'logged-out visitors reached the private audit log';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
set role authenticated;
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.post) <> 1 then raise exception 'signed-in bob should read the public post'; end if;
  begin perform count(*) from public.audit_log; raise exception 'bob reached the private audit log';
  exception when insufficient_privilege then null; end;
end $$;

-- Bob tries to change and delete Alice's post. Both "succeed" with 0 rows; the readback proves it.
update public.post set body = 'defaced' where id = '11111111-0000-0000-0000-000000000000';
delete from public.post where id = '11111111-0000-0000-0000-000000000000';
do $$ begin
  begin insert into public.post (author, body) values ('a0000000-0000-0000-0000-000000000000', 'forged as alice'); raise exception 'bob posted as alice';
  exception when insufficient_privilege then null; end;
end $$;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select body from public.post where id = '11111111-0000-0000-0000-000000000000') is distinct from 'hello' then raise exception 'bob changed or deleted alice''s post'; end if;
  if (select count(*) from public.post) <> 1 then raise exception 'a forged post exists'; end if;
end $$;

-- Alice creates, changes and deletes her own posts. After each write, read back as alice, bob and anon.
insert into public.post (id, author, body) values ('22222222-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 'mine');
do $$ begin
  if (select body from public.post where id = '22222222-0000-0000-0000-000000000000') is distinct from 'mine' then raise exception 'alice cannot read back her new post'; end if;
end $$;
update public.post set body = 'edited' where id = '22222222-0000-0000-0000-000000000000';
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select body from public.post where id = '22222222-0000-0000-0000-000000000000') is distinct from 'edited' then raise exception 'bob should see alice''s edited public post'; end if;
end $$;
reset role;
set role anon;
do $$ begin
  if (select body from public.post where id = '22222222-0000-0000-0000-000000000000') is distinct from 'edited' then raise exception 'logged-out visitors should see the edited public post'; end if;
end $$;
reset role;
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
delete from public.post where id = '22222222-0000-0000-0000-000000000000';
do $$ begin
  if exists (select 1 from public.post where id = '22222222-0000-0000-0000-000000000000') then raise exception 'alice''s delete did not take effect'; end if;
end $$;
reset role;
do $$ begin
  if exists (select 1 from public.post where id = '22222222-0000-0000-0000-000000000000') then raise exception 'the deleted post still exists'; end if;
end $$;
