-- The transfer hole without its `when` guard: anyone signed in could move anyone's money.
create or replace function "svetra_holes"."transfer"(p_caller uuid, p_source uuid, p_destination uuid, p_amount numeric(18, 6))
  returns "public"."transaction" language plpgsql security definer set search_path = ''
  as $svetra$
begin
  raise exception 'transfer is not written yet: edit supabase/holes/transfer.sql';
end
$svetra$;
