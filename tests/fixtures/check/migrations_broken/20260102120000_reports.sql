-- Fails on purpose: a table that was never created by a migration.
create policy reports_read on public.legacy_orders for select using (true);
