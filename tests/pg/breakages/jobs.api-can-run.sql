-- The scheduled body made executable by signed-in people, who could then run it, with any caller id.
grant execute on function "svetra_holes"."expire_invites"(uuid) to authenticated;
