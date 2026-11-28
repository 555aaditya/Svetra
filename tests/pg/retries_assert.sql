-- Behaviour of retry keys: a repeat returns the first answer and runs nothing, changed inputs are
-- refused, the guard runs again on a repeat, a failed call leaves no key, keys belong to one person,
-- and the key table is closed to the app.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000'), ('b0000000-0000-0000-0000-000000000000');
insert into public.wallet (id, owner, balance) values
  ('11111111-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 10),
  ('22222222-0000-0000-0000-000000000000', 'b0000000-0000-0000-0000-000000000000', 10);
create temp table seen (name text primary key, val text);
grant all on seen to authenticated, anon;

set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';

-- The first call runs and returns a payment.
insert into seen select 'first', (public.pay('cccccccc-0000-0000-0000-000000000001', '11111111-0000-0000-0000-000000000000', 5)).id::text;
do $$ begin
  if (select balance from public.wallet where id = '11111111-0000-0000-0000-000000000000') <> 15 then raise exception 'the first call should have run'; end if;
end $$;

-- A repeat returns the same payment, and nothing runs again.
insert into seen select 'repeat', (public.pay('cccccccc-0000-0000-0000-000000000001', '11111111-0000-0000-0000-000000000000', 5)).id::text;
do $$ begin
  if (select val from seen where name = 'first') is distinct from (select val from seen where name = 'repeat') then raise exception 'a repeat should return the first payment'; end if;
  if (select balance from public.wallet where id = '11111111-0000-0000-0000-000000000000') <> 15 then raise exception 'a repeat ran the body again'; end if;
end $$;
reset role;
do $$ begin
  if (select count(*) from public.payment) <> 1 then raise exception 'a repeat made a second payment'; end if;
  if (select count(*) from svetra.audit_log where kind = 'call' and subject = 'pay') <> 1 then raise exception 'a repeat should not be audited again'; end if;
end $$;

set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ begin
  -- The same key with other inputs is refused, and says why.
  begin perform public.pay('cccccccc-0000-0000-0000-000000000001', '11111111-0000-0000-0000-000000000000', 6); raise exception 'changed inputs were accepted';
  exception when invalid_parameter_value then
    if sqlerrm <> 'pay: request was already used with different inputs' then raise exception 'wrong message: %', sqlerrm; end if;
  end;
  -- No key at all is refused too, so a client can't think it is protected when it isn't.
  begin perform public.pay(null, '11111111-0000-0000-0000-000000000000', 5); raise exception 'a null key was accepted';
  exception when invalid_parameter_value then
    if sqlerrm <> 'pay: request is needed so a repeat can be recognised' then raise exception 'wrong message: %', sqlerrm; end if;
  end;
  -- A call that fails leaves no key: the bad amount is refused, then the same key works.
  begin perform public.pay('cccccccc-0000-0000-0000-000000000002', '11111111-0000-0000-0000-000000000000', 0); raise exception 'a bad amount was accepted';
  exception when invalid_parameter_value then
    if sqlerrm <> 'pay: amount > 0' then raise exception 'wrong message: %', sqlerrm; end if;
  end;
end $$;
select public.pay('cccccccc-0000-0000-0000-000000000002', '11111111-0000-0000-0000-000000000000', 3);
do $$ begin
  if (select balance from public.wallet where id = '11111111-0000-0000-0000-000000000000') <> 18 then raise exception 'the retry after a failed call should have run once'; end if;
end $$;

-- Keys belong to one person: bob using alice's key value runs his own call, and never sees hers.
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
select public.pay('cccccccc-0000-0000-0000-000000000001', '22222222-0000-0000-0000-000000000000', 7);
do $$ begin
  if (select balance from public.wallet where id = '22222222-0000-0000-0000-000000000000') <> 17 then raise exception 'bob''s call with the same key value should have run'; end if;
  -- Bob can't use a key to reach alice's wallet either: the guard refuses him, key or no key.
  begin perform public.pay('cccccccc-0000-0000-0000-000000000009', '11111111-0000-0000-0000-000000000000', 1); raise exception 'bob paid into alice''s wallet';
  exception when insufficient_privilege then null; end;
end $$;

-- The key table is closed to the app.
do $$ begin
  begin perform count(*) from svetra.hole_keys; raise exception 'bob read the keys';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
set role anon;
do $$ begin
  begin perform public.pay('cccccccc-0000-0000-0000-000000000001', '11111111-0000-0000-0000-000000000000', 5); raise exception 'a visitor called pay';
  exception when insufficient_privilege then null; end;
end $$;
reset role;

-- The guard runs again on a repeat: once alice's access is gone, her repeat can't read the stored answer.
update public.wallet set owner = 'b0000000-0000-0000-0000-000000000000' where id = '11111111-0000-0000-0000-000000000000';
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ begin
  begin perform public.pay('cccccccc-0000-0000-0000-000000000001', '11111111-0000-0000-0000-000000000000', 5); raise exception 'a repeat read the answer after access was lost';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
update public.wallet set owner = 'a0000000-0000-0000-0000-000000000000' where id = '11111111-0000-0000-0000-000000000000';

-- A hole with no output and a text key: the repeat does nothing, even after the data changed.
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
select public.relabel('k1', '11111111-0000-0000-0000-000000000000', 'first');
reset role;
update public.wallet set label = 'edited' where id = '11111111-0000-0000-0000-000000000000';
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
select public.relabel('k1', '11111111-0000-0000-0000-000000000000', 'first');
do $$ begin
  if (select label from public.wallet where id = '11111111-0000-0000-0000-000000000000') <> 'edited' then raise exception 'a repeat of relabel ran the body again'; end if;
end $$;

-- A scalar answer comes back as the same value and type.
insert into seen select 'total1', public.total('t1', '11111111-0000-0000-0000-000000000000')::text;
reset role;
update public.wallet set balance = 999 where id = '11111111-0000-0000-0000-000000000000';
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
insert into seen select 'total2', public.total('t1', '11111111-0000-0000-0000-000000000000')::text;
do $$ begin
  if (select val from seen where name = 'total1') is distinct from (select val from seen where name = 'total2') then raise exception 'a repeat of total should return the first balance'; end if;
  if pg_typeof(public.total('t1', '11111111-0000-0000-0000-000000000000'))::text <> 'numeric' then raise exception 'the answer should keep its type'; end if;
end $$;
reset role;
