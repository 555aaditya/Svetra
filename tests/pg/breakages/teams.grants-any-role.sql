-- A policy widened by hand: the add_member hole can now grant any role, including admin.
drop policy "svetra_hole_add_member" on "svetra"."role_grants";
create policy "svetra_hole_add_member" on "svetra"."role_grants" for all to "svetra_hole_add_member" using (true) with check (true);
