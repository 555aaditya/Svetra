-- Behaviour of the validated example: bad input is refused as 22023 after the guard, the first
-- failing condition is named, a null fails, a % in a condition is text, and a caller the guard
-- refuses never learns the rules.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000'), ('b0000000-0000-0000-0000-000000000000');
insert into public.wallet (id, owner, balance, label) values ('11111111-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 10, 'main');

set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';

-- Alice, the owner: a good amount runs the body.
select public.deposit('11111111-0000-0000-0000-000000000000', 5);
do $$ begin
  if (select balance from public.wallet) <> 15 then raise exception 'a valid deposit should have run'; end if;
end $$;

-- Each bad amount is refused as bad input, and names the condition that failed.
do $$ begin
  begin perform public.deposit('11111111-0000-0000-0000-000000000000', 0); raise exception 'zero was accepted';
  exception when invalid_parameter_value then
    if sqlerrm <> 'deposit: amount > 0' then raise exception 'wrong message: %', sqlerrm; end if;
  end;
  begin perform public.deposit('11111111-0000-0000-0000-000000000000', -3); raise exception 'a negative amount was accepted';
  exception when invalid_parameter_value then null; end;
  begin perform public.deposit('11111111-0000-0000-0000-000000000000', 10001); raise exception 'too large was accepted';
  exception when invalid_parameter_value then
    if sqlerrm <> 'deposit: amount <= 10000' then raise exception 'wrong message: %', sqlerrm; end if;
  end;
  -- A null amount fails, where a SQL check would let it pass.
  begin perform public.deposit('11111111-0000-0000-0000-000000000000', null); raise exception 'a null amount was accepted';
  exception when invalid_parameter_value then null; end;
  -- A % in the condition is text, not a placeholder.
  begin perform public.relabel('11111111-0000-0000-0000-000000000000', '100%'); raise exception 'the refused label was accepted';
  exception when invalid_parameter_value then
    if sqlerrm <> 'relabel: label != null and label != ''100%''' then raise exception 'wrong message: %', sqlerrm; end if;
  end;
  begin perform public.relabel('11111111-0000-0000-0000-000000000000', null); raise exception 'a null label was accepted';
  exception when invalid_parameter_value then null; end;
end $$;
select public.relabel('11111111-0000-0000-0000-000000000000', 'savings');

-- Nothing the refused calls tried was kept.
do $$ begin
  if (select balance from public.wallet) <> 15 then raise exception 'a refused deposit changed the balance'; end if;
end $$;

-- Bob isn't allowed: he gets "not allowed" (42501) even with a bad amount, so the rules stay private.
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  begin perform public.deposit('11111111-0000-0000-0000-000000000000', 0); raise exception 'bob reached the validation';
  exception when insufficient_privilege then null; end;
end $$;
reset role;

-- A condition between two inputs: moving a wallet to itself is refused, to another is not.
insert into public.wallet (id, owner, balance) values ('22222222-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 1);
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
select public.move('11111111-0000-0000-0000-000000000000', '22222222-0000-0000-0000-000000000000');
do $$ begin
  begin perform public.move('11111111-0000-0000-0000-000000000000', '11111111-0000-0000-0000-000000000000'); raise exception 'a wallet was moved onto itself';
  exception when invalid_parameter_value then
    if sqlerrm <> 'move: source != destination' then raise exception 'wrong message: %', sqlerrm; end if;
  end;
end $$;
reset role;
