-- Behaviour of the compiled jobs example: the job is registered as declared, runs the body, and
-- nobody calling the API can run that body. pg_cron itself isn't here (the stand-in has a stub
-- cron schema); scripts/supabase-check.sh covers the real extension.
insert into auth.users values ('a0000000-0000-0000-0000-000000000000');
insert into public.invite (owner, email, status, expires_at) values
  ('a0000000-0000-0000-0000-000000000000', 'old@example.com', 'pending', now() - interval '1 day'),
  ('a0000000-0000-0000-0000-000000000000', 'fresh@example.com', 'pending', now() + interval '1 day'),
  ('a0000000-0000-0000-0000-000000000000', 'done@example.com', 'accepted', now() - interval '1 day');

do $$ begin
  if (select count(*) from cron.job where jobname like 'svetra\_%') <> 1 then raise exception 'expected exactly one svetra job'; end if;
end $$;

-- What the clock runs: the job's own command, as the migration's role. Twice, because a body must be
-- safe to run twice.
do $$ declare cmd text; begin
  select command into cmd from cron.job where jobname = 'svetra_expire_invites';
  execute cmd;
  execute cmd;
  if (select status from public.invite where email = 'old@example.com') <> 'expired' then raise exception 'the job did not expire the old invite'; end if;
  if (select status from public.invite where email = 'fresh@example.com') <> 'pending' then raise exception 'the job expired an invite that is still valid'; end if;
  if (select status from public.invite where email = 'done@example.com') <> 'accepted' then raise exception 'the job changed an accepted invite'; end if;
end $$;

-- A second connection holding the lock stops a run: the job asks for it by the hole's name.
do $$ declare cmd text; begin
  select command into cmd from cron.job where jobname = 'svetra_expire_invites';
  if position('pg_try_advisory_xact_lock(hashtext(''svetra_expire_invites''))' in cmd) = 0 then raise exception 'the job does not take the lock'; end if;
end $$;

-- Nobody calling the API can run it.
do $$ declare r text; begin
  foreach r in array array['anon', 'authenticated'] loop
    execute format('set role %I', r);
    begin
      perform "svetra_holes"."expire_invites"(null);
      raise exception '% ran the scheduled body', r;
    exception when insufficient_privilege then null;
    end;
    reset role;
  end loop;
end $$;
