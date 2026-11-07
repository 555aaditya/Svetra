-- A policy written by hand, outside svetra.yaml, that lets every signed-in person read every booking.
create policy svetra_ci_leak on public.booking for select to authenticated using (true);
