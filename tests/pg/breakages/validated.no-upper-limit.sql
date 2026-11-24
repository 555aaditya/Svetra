-- The deposit hole without its second requirement: a deposit of any size would go through.
create or replace function "svetra_holes"."deposit"(p_caller uuid, p_wallet uuid, p_amount numeric(18, 2))
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce((select t."owner" from "public"."wallet" t where t."id" = p_wallet) = p_caller, false) then
    raise exception 'not allowed to call deposit' using errcode = '42501';
  end if;
  if not coalesce(p_amount > 0, false) then
    raise exception using message = 'deposit: amount > 0', errcode = '22023';
  end if;
  raise exception 'deposit is not written yet: edit supabase/holes/deposit.sql';
end
$svetra$;
