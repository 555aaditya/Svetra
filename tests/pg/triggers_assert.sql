-- Behaviour of the compiled triggers example, with real hole bodies: each event fires its hole
-- for the right rows only, whoever makes the change fires it, the body runs as its own role, the
-- caller reaches the body, nobody calling the API can run either function, and a loop that slips
-- past validation ends in an error. Each part is undone, so the data the generated tests find is
-- the data they expect.

insert into auth.users values ('a0000000-0000-0000-0000-000000000001'), ('a0000000-0000-0000-0000-000000000002');
insert into public.customer (id, owner, name) values
  ('c0000000-0000-0000-0000-000000000001', 'a0000000-0000-0000-0000-000000000001', 'Acme');

-- Insert, update and delete as a signed-in person, who can't run anything in svetra_holes. The
-- notifications are read back as the database owner, after each step.
begin;
select set_config('request.jwt.claim.sub', 'a0000000-0000-0000-0000-000000000001', true);
set local role authenticated;
-- WHEN `total > 0` is false for the first order: it records nothing.
insert into public.order (id, owner, customer, total) values ('d0000000-0000-0000-0000-000000000001', 'a0000000-0000-0000-0000-000000000001', 'c0000000-0000-0000-0000-000000000001', 0);
insert into public.order (id, owner, customer, total) values ('d0000000-0000-0000-0000-000000000002', 'a0000000-0000-0000-0000-000000000001', 'c0000000-0000-0000-0000-000000000001', 5);
reset role;
select coalesce(array_agg(message order by message collate "C"), '{}') = array['An order was placed'] as ok from public.notification \gset
\if :ok
\else
  \echo 'placing orders did not record exactly one notification, for the order with a total'
  select 1/0;
\endif
-- Only a change to paid fires notify_paid: open -> shipped, and paid -> paid, don't.
set local role authenticated;
update public.order set status = 'shipped' where id = 'd0000000-0000-0000-0000-000000000002';
update public.order set status = 'paid' where id = 'd0000000-0000-0000-0000-000000000002';
update public.order set status = 'paid', total = 6 where id = 'd0000000-0000-0000-0000-000000000002';
reset role;
select coalesce(array_agg(message order by message collate "C"), '{}') = array['An order was placed', 'Order for Acme is paid'] as ok from public.notification \gset
\if :ok
\else
  \echo 'notify_paid did not fire exactly once, when the status became paid'
  select 1/0;
\endif
set local role authenticated;
delete from public.order where id = 'd0000000-0000-0000-0000-000000000001';
delete from public.order where id = 'd0000000-0000-0000-0000-000000000002';
reset role;
select coalesce(array_agg(message order by message collate "C"), '{}') = array['An order was deleted', 'An order was placed', 'Order for Acme is paid'] as ok from public.notification \gset
\if :ok
\else
  \echo 'deleting an order with a total did not record exactly one notification'
  select 1/0;
\endif
rollback;

-- The body runs as its own role, and gets the person who made the change as p_caller (null for
-- a change made with no signed-in person).
begin;
create or replace function svetra_holes.note_order_change(p_caller uuid, p_old public.order, p_new public.order)
  returns void language plpgsql security definer set search_path = '' as $$
begin
  insert into public.notification (owner, message)
    values ('a0000000-0000-0000-0000-000000000002', format('as %s, caller %s, old %s, new %s', current_user, p_caller, p_old is null, p_new is null));
end $$;
select set_config('request.jwt.claim.sub', 'a0000000-0000-0000-0000-000000000001', true);
set local role authenticated;
insert into public.order (id, owner, customer, total) values ('d0000000-0000-0000-0000-000000000003', 'a0000000-0000-0000-0000-000000000001', 'c0000000-0000-0000-0000-000000000001', 5);
reset role;
select set_config('request.jwt.claim.sub', '', true);
insert into public.order (id, owner, customer, total) values ('d0000000-0000-0000-0000-000000000004', 'a0000000-0000-0000-0000-000000000001', 'c0000000-0000-0000-0000-000000000001', 5);
select coalesce(array_agg(message order by message collate "C"), '{}') = array[
  'as svetra_hole_note_order_change, caller , old t, new f',
  'as svetra_hole_note_order_change, caller a0000000-0000-0000-0000-000000000001, old t, new f'] as ok
  from public.notification where owner = 'a0000000-0000-0000-0000-000000000002' \gset
\if :ok
\else
  \echo 'the body did not run as its own role with the right p_caller'
  select 1/0;
\endif
rollback;

-- Nobody calling the API can run the body or the function that fires it.
do $$ declare r text; f text; begin
  foreach r in array array['anon', 'authenticated'] loop
    foreach f in array array['svetra_holes.notify_paid__fire()', 'svetra_holes.notify_paid(uuid, public.order, public.order)'] loop
      if has_function_privilege(r, f, 'execute') then raise exception '% can run %', r, f; end if;
    end loop;
    execute format('set role %I', r);
    begin
      perform "svetra_holes"."notify_paid__fire"();
      raise exception '% ran the trigger function', r;
    exception when insufficient_privilege then null;
    end;
    reset role;
  end loop;
end $$;

-- A loop through a trigger Svetra can't see (made by hand) ends at the depth limit.
begin;
create function public.hand_made() returns trigger language plpgsql security definer set search_path = '' as $$
begin
  insert into public.order (owner, customer, total) values (new.owner, 'c0000000-0000-0000-0000-000000000001', 7);
  return null;
end $$;
create trigger hand_made after insert on public.notification for each row execute function public.hand_made();
do $$ begin
  begin
    insert into public.order (owner, customer, total) values ('a0000000-0000-0000-0000-000000000001', 'c0000000-0000-0000-0000-000000000001', 7);
    raise exception 'the loop did not stop';
  exception when others then
    if sqlstate <> '54001' then raise; end if;
  end;
end $$;
rollback;

delete from public.customer;
delete from auth.users;
