-- A hand-added policy that lets any signed-in person insert a profile for anyone.
create policy leak on public.profile for insert to authenticated with check (true);
