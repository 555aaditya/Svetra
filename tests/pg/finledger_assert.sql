-- Behaviour of the compiled ledger. Every check raises an exception when it fails.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000'), ('b0000000-0000-0000-0000-000000000000'), ('c0000000-0000-0000-0000-000000000000');
insert into public.account (id, owner, currency, balance) values
  ('a1000000-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 'USD', 100),
  ('b1000000-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000000', 'USD', 50);
insert into public.transaction (source, destination, amount) values ('a1000000-0000-0000-0000-000000000000', 'b1000000-0000-0000-0000-000000000000', 10);

-- Constraints, as the table owner.
do $$ begin
  begin insert into public.account (owner, currency, balance) values ('a0000000-0000-0000-0000-000000000000', 'EUR', -1); raise exception 'negative balance accepted';
  exception when check_violation then null; end;
  begin insert into public.account (owner, currency, balance) values ('a0000000-0000-0000-0000-000000000000', 'JPY', 1); raise exception 'unknown currency accepted';
  exception when check_violation then null; end;
  begin insert into public.account (owner, currency, balance) values ('a0000000-0000-0000-0000-000000000000', 'USD', 1); raise exception 'second USD account accepted';
  exception when unique_violation then null; end;
  begin insert into public.transaction (source, destination, amount) values ('a1000000-0000-0000-0000-000000000000', 'a1000000-0000-0000-0000-000000000000', 1); raise exception 'self-transfer accepted';
  exception when check_violation then null; end;
  begin insert into public.transaction (source, destination, amount) values (gen_random_uuid(), 'a1000000-0000-0000-0000-000000000000', 1); raise exception 'unknown account accepted';
  exception when foreign_key_violation then null; end;
end $$;

-- Row-level security, as each user.
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.account) <> 1 then raise exception 'alice should see 1 account'; end if;
  if (select count(*) from public.transaction) <> 1 then raise exception 'alice should see the transaction she sent'; end if;
  begin insert into public.account (owner, currency, balance) values ('a0000000-0000-0000-0000-000000000000', 'EUR', 1000000); raise exception 'alice created an account';
  exception when insufficient_privilege then null; end;
  update public.account set balance = 1000000;
  if (select balance from public.account) <> 100 then raise exception 'alice changed her balance directly'; end if;
end $$;
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select balance from public.account) <> 50 then raise exception 'alice''s update changed bob''s balance'; end if;
  if (select count(*) from public.account) <> 1 then raise exception 'bob should see 1 account'; end if;
  if (select count(*) from public.transaction) <> 1 then raise exception 'bob should see the transaction he received'; end if;
end $$;
set request.jwt.claim.sub = 'c0000000-0000-0000-0000-000000000000';
do $$ begin
  if (select count(*) from public.account) + (select count(*) from public.transaction) <> 0 then raise exception 'carol should see nothing'; end if;
end $$;
reset role;
set role anon;
do $$ begin
  if (select count(*) from public.account) <> 0 then raise exception 'anon should see nothing'; end if;
end $$;
reset role;

-- Read back as the table owner, bypassing row-level security: the blocked writes left no trace.
do $$ begin
  if (select count(*) from public.account) <> 2 then raise exception 'a blocked insert created an account'; end if;
  if (select sum(balance) from public.account) <> 150 then raise exception 'a blocked update changed a balance'; end if;
end $$;

-- The transfer hole, with the real body in tests/fixtures/holes/finledger/transfer.sql.
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ declare t public.transaction; begin
  t := public.transfer('a1000000-0000-0000-0000-000000000000', 'b1000000-0000-0000-0000-000000000000', 30);
  if t.amount <> 30 or t.status <> 'committed' then raise exception 'transfer returned the wrong transaction'; end if;
end $$;
-- Bob can't move alice's money: the guard refuses him before the body runs.
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  perform public.transfer('a1000000-0000-0000-0000-000000000000', 'b1000000-0000-0000-0000-000000000000', 30);
  raise exception 'bob moved alice''s money';
exception when insufficient_privilege then null; end $$;
reset role;
set role anon;
do $$ begin
  perform public.transfer('a1000000-0000-0000-0000-000000000000', 'b1000000-0000-0000-0000-000000000000', 30);
  raise exception 'a logged-out visitor called transfer';
exception when insufficient_privilege then null; end $$;
reset role;
do $$ begin
  if (select balance from public.account where id = 'a1000000-0000-0000-0000-000000000000') <> 70 then raise exception 'alice should have 70 after the transfer'; end if;
  if (select balance from public.account where id = 'b1000000-0000-0000-0000-000000000000') <> 80 then raise exception 'bob should have 80 after the transfer'; end if;
  if (select count(*) from public.transaction) <> 2 then raise exception 'the transfer should be recorded once'; end if;
end $$;
