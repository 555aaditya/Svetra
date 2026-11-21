-- Behaviour of the audited example: row changes and hole calls reach the log with the actor,
-- the app can't touch the log, and nobody can change or remove a record.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000'), ('b0000000-0000-0000-0000-000000000000');

-- Alice writes through the API: insert, update, delete are each logged with her id.
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
insert into public.note (id, owner, body) values ('11111111-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 'one');
update public.note set body = 'two' where id = '11111111-0000-0000-0000-000000000000';
insert into public.note (id, owner, body) values ('22222222-0000-0000-0000-000000000000', 'a0000000-0000-0000-0000-000000000000', 'keep');

-- The app can't read, write or change the log, whoever asks.
do $$ begin
  begin perform count(*) from svetra.audit_log; raise exception 'alice reached the audit log';
  exception when insufficient_privilege then null; end;
  begin insert into svetra.audit_log (kind, subject, action) values ('row', 'note', 'insert'); raise exception 'alice wrote to the audit log';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
set role anon;
do $$ begin
  begin perform count(*) from svetra.audit_log; raise exception 'a visitor reached the audit log';
  exception when insufficient_privilege then null; end;
end $$;
reset role;

-- A refused row write leaves no record: bob can't change alice's note.
set role authenticated;
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
update public.note set body = 'defaced' where id = '11111111-0000-0000-0000-000000000000';
reset role;
do $$ begin
  if (select count(*) from svetra.audit_log where kind = 'row' and subject = 'note') <> 3 then raise exception 'expected 3 row records (insert, update, insert), no record for a write that changed nothing'; end if;
  if (select actor from svetra.audit_log where action = 'update') is distinct from 'a0000000-0000-0000-0000-000000000000'::uuid then raise exception 'the update should name alice'; end if;
  if (select old->>'body' || '>' || (new->>'body') from svetra.audit_log where action = 'update') <> 'one>two' then raise exception 'the update should hold the row before and after'; end if;
  if (select old is null from svetra.audit_log where action = 'insert' order by id limit 1) is not true then raise exception 'an insert has no old row'; end if;
end $$;

-- Bob calls archive on alice's note: refused, and nothing is recorded, not even the call.
set role authenticated;
set request.jwt.claim.sub = 'b0000000-0000-0000-0000-000000000000';
do $$ begin
  begin perform public.archive('11111111-0000-0000-0000-000000000000'); raise exception 'bob archived alice''s note';
  exception when insufficient_privilege then null; end;
end $$;
reset role;
do $$ begin
  if exists (select 1 from svetra.audit_log where kind = 'call') then raise exception 'a refused call must leave no record'; end if;
end $$;

-- Alice archives her own note: the call (with its input) and the delete it caused are both logged.
set role authenticated;
set request.jwt.claim.sub = 'a0000000-0000-0000-0000-000000000000';
select public.archive('11111111-0000-0000-0000-000000000000');
select public.sweep();
reset role;
do $$ begin
  if (select count(*) from svetra.audit_log where kind = 'call') <> 2 then raise exception 'expected two call records'; end if;
  if (select new->>'note' from svetra.audit_log where subject = 'archive') <> '11111111-0000-0000-0000-000000000000' then raise exception 'the call should hold its input'; end if;
  if (select new from svetra.audit_log where subject = 'sweep') <> '{}'::jsonb then raise exception 'a hole with no input logs an empty object'; end if;
  if (select actor from svetra.audit_log where subject = 'archive') is distinct from 'a0000000-0000-0000-0000-000000000000'::uuid then raise exception 'the call should name alice'; end if;
  if (select count(*) from svetra.audit_log where action = 'delete' and actor = 'a0000000-0000-0000-0000-000000000000') <> 2 then raise exception 'both deletes made by the holes should be logged with alice as actor'; end if;
  if exists (select 1 from public.note) then raise exception 'the holes should have deleted the notes'; end if;
end $$;

-- Append-only: not even the owner can change or remove a record, or empty the log.
do $$ begin
  begin update svetra.audit_log set actor = null; raise exception 'a record was changed';
  exception when insufficient_privilege then null; end;
  begin delete from svetra.audit_log; raise exception 'a record was deleted';
  exception when insufficient_privilege then null; end;
  begin truncate svetra.audit_log; raise exception 'the log was truncated';
  exception when insufficient_privilege then null; end;
end $$;

-- Hole roles have no way in either.
do $$ begin
  if has_table_privilege('svetra_hole_archive', 'svetra.audit_log', 'select,insert,update,delete') then raise exception 'a hole role holds a privilege on the log'; end if;
end $$;
