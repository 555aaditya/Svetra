-- The relabel hole with no requirement at all: a missing label, or a label of "100%", would go through.
create or replace function "svetra_holes"."relabel"(p_caller uuid, p_wallet uuid, p_label text)
  returns void language plpgsql security definer set search_path = ''
  as $svetra$
begin
  if not coalesce((select t."owner" from "public"."wallet" t where t."id" = p_wallet) = p_caller, false) then
    raise exception 'not allowed to call relabel' using errcode = '42501';
  end if;
  raise exception 'relabel is not written yet: edit supabase/holes/relabel.sql';
end
$svetra$;
