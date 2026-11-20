-- Two views over those tables.
create view public.user_emails as select id, email from auth.users;                         -- view_bypass, critical
grant select on public.user_emails to anon, authenticated;
create view public.order_totals as select customer, sum(total) as total from public.orders group by customer; -- view_bypass
grant select on public.order_totals to authenticated;
