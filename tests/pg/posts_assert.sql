-- Behaviour of the compiled posts example: a private table, a four-action policy and an anon policy.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000');
insert into public.post (author, body) values ('a0000000-0000-0000-0000-000000000000', 'hello');
set role anon;
do $$ begin
  if (select count(*) from public.post) <> 1 then raise exception 'anon should read posts'; end if;
  begin perform count(*) from public.audit_log; raise exception 'anon reached the private audit log';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ begin
  insert into public.post (author, body) values ('a0000000-0000-0000-0000-000000000000', 'mine');
  begin insert into public.post (author, body) values (gen_random_uuid(), 'forged'); raise exception 'post forged as another author';
  exception when insufficient_privilege then null; end;
  begin perform count(*) from public.audit_log; raise exception 'a user reached the private audit log';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
