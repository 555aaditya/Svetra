  -- Safe to run twice and to run late: it only moves invites that are still pending.
  update "public"."invite"
    set "status" = 'expired'
    where "status" = 'pending' and "expires_at" < now();
