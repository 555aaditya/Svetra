-- The trigger function made executable by signed-in people: nobody in the API should be able to run it.
grant execute on function "svetra_holes"."notify_paid__fire"() to authenticated;
