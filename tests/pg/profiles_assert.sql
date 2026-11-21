-- Behaviour of the compiled profiles example: a table whose primary key is the auth user, so each
-- person has at most one row, and only their own. Rows are read back after every write.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000'), ('b0000000-0000-0000-0000-000000000000');

set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
insert into public.profile (id, username) values ('a0000000-0000-0000-0000-000000000000', 'alice');
do $$ begin
  -- A second profile for the same person is a duplicate key, whoever tries it.
  begin insert into public.profile (id, username) values ('a0000000-0000-0000-0000-000000000000', 'again'); raise exception 'alice got two profiles';
  exception when unique_violation then null; end;
  -- Nobody adds a profile for someone else.
  begin insert into public.profile (id, username) values ('b0000000-0000-0000-0000-000000000000', 'forged'); raise exception 'alice added bob''s profile';
  exception when insufficient_privilege then null; end;
end $$;

set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
update public.profile set username = 'defaced' where id = 'a0000000-0000-0000-0000-000000000000';
delete from public.profile where id = 'a0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select username from public.profile) <> 'alice' then raise exception 'bob changed or deleted alice''s profile'; end if;
end $$;
insert into public.profile (id, username) values ('b0000000-0000-0000-0000-000000000000', 'bob');
do $$ begin
  if (select count(*) from public.profile) <> 2 then raise exception 'bob should be able to add his own profile'; end if;
end $$;
reset role;
