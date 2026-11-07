-- has_role without its second-factor condition: an admin who signed in with a password alone
-- would count as admin. Exactly the checks for the admin without a second factor must fail.
create or replace function "svetra"."has_role"(p_role text, p_scope uuid default null)
  returns boolean language sql stable security definer set search_path = ''
  as $$ select exists (
    select 1 from "svetra"."role_grants" g
    where g."user_id" = (select auth.uid()) and g."role" = p_role and g."scope_id" is not distinct from p_scope
  ) $$;
