-- Behaviour of webhook holes: only the service role calls them, a call does its work as the hole's
-- role, a repeat delivery is ignored, a refused or failed call leaves no record of the event (so the
-- sender's retry runs), and the events table is closed to everyone.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000');
insert into public.invoice (id, owner, amount) values ('11111111-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 50);

-- Nobody signed in, or out, can run it.
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
do $$ begin
  begin perform public.payment_received('11111111-0000-0000-0000-000000000000', 50, 'evt1'); raise exception 'a signed-in person called the webhook';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
set role anon;
do $$ begin
  begin perform public.payment_received('11111111-0000-0000-0000-000000000000', 50, 'evt1'); raise exception 'a visitor called the webhook';
  exception when insufficient_privilege then null; end;
end $$;
reset role;

-- The service role's call does the work. Its key carries no user, so the claim is cleared.
reset request.jwt.claim.sub;
set role service_role;
select public.payment_received('11111111-0000-0000-0000-000000000000', 50, 'evt1');
reset role;
do $$ begin
  if (select count(*) from public.payment) <> 1 then raise exception 'the call should have made one payment'; end if;
  if (select status from public.invoice) <> 'paid' then raise exception 'the call should have marked the invoice paid'; end if;
  if (select count(*) from svetra.audit_log where subject = 'payment_received' and actor is null) <> 1 then raise exception 'the call should be audited, with no actor'; end if;
end $$;

-- The same event again is ignored: no second payment, no second audit record.
set role service_role;
select public.payment_received('11111111-0000-0000-0000-000000000000', 50, 'evt1');
reset role;
do $$ begin
  if (select count(*) from public.payment) <> 1 then raise exception 'a repeat delivery made a second payment'; end if;
  if (select count(*) from svetra.audit_log where subject = 'payment_received') <> 1 then raise exception 'a repeat delivery was audited again'; end if;
end $$;

-- A `when` that refuses leaves no record of the event, so the retry can run once it holds.
set role service_role;
do $$ begin
  begin perform public.payment_received('11111111-0000-0000-0000-000000000000', 0, 'evt2'); raise exception 'a payment of 0 was accepted';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
do $$ begin
  if exists (select from svetra.webhook_events where event_id = 'evt2') then raise exception 'a refused call left its event behind'; end if;
end $$;

-- So does a body that fails: an invoice that doesn't exist breaks the foreign key.
set role service_role;
do $$ begin
  begin perform public.payment_received('99999999-0000-0000-0000-000000000000', 5, 'evt3'); raise exception 'a payment for a missing invoice was accepted';
  exception when foreign_key_violation then null; end;
end $$;
reset role;
do $$ begin
  if exists (select from svetra.webhook_events where event_id = 'evt3') then raise exception 'a failed call left its event behind'; end if;
  if (select count(*) from public.payment) <> 1 then raise exception 'a failed call left a payment'; end if;
end $$;

-- The same event id in another webhook is its own event.
do $$ begin
  if (select count(*) from svetra.webhook_events) <> 1 then raise exception 'only evt1 should be recorded'; end if;
end $$;

-- A webhook with no dedupe runs every time, and its requirement still applies.
set role service_role;
select public.form_submitted('11111111-0000-0000-0000-000000000000', 'hello');
select public.form_submitted('11111111-0000-0000-0000-000000000000', 'hello');
do $$ begin
  begin perform public.form_submitted('11111111-0000-0000-0000-000000000000', ''); raise exception 'an empty note was accepted';
  exception when invalid_parameter_value then null; end;
end $$;
reset role;

-- The events table is closed to the app and to the service role itself.
set role service_role;
do $$ begin
  begin perform count(*) from svetra.webhook_events; raise exception 'the service role read the events';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
set role authenticated;
do $$ begin
  begin perform count(*) from svetra.webhook_events; raise exception 'a signed-in person read the events';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
