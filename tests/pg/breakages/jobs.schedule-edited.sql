-- The job's schedule changed by hand, from every day at 03:00 to every minute.
select cron.schedule('svetra_expire_invites', '* * * * *', $job$select "svetra_holes"."expire_invites"(null)
    where pg_try_advisory_xact_lock(hashtext('svetra_expire_invites'))$job$);
