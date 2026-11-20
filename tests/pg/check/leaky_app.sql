-- A small app with one of each leak `svetra check` looks for (docs/check.md), as an AI app
-- builder might write it. Applied after src/standin/base.sql.
create table public.orders (id uuid primary key, customer uuid, total numeric);             -- rls_off
create table public.posts (id uuid primary key, author uuid, body text);
alter table public.posts enable row level security;
create policy public_read on public.posts for select using (true);                          -- open_read (to public)
create table public.comments (id uuid primary key, post uuid, body text);
alter table public.comments enable row level security;
create policy anyone_edits on public.comments for all to anon using (true) with check (true); -- open_write
create policy service_manages on public.comments for all using (true);                      -- open_write, meant for the service role
create table public.notes (id uuid primary key, owner uuid, body text);
alter table public.notes enable row level security;
create policy notes_signed_in on public.notes for update to authenticated using (true);    -- signed_in_write
create policy admins_read on public.notes for select to authenticated
  using (((auth.jwt() -> 'user_metadata') ->> 'role') = 'admin');                          -- user_metadata
create table public.audit (id uuid primary key, note text);
alter table public.audit enable row level security;                                         -- no_policy
create view public.user_emails as select id, email from auth.users;                         -- view_bypass, critical
grant select on public.user_emails to anon, authenticated;
create view public.order_totals as select customer, sum(total) as total from public.orders group by customer; -- view_bypass
grant select on public.order_totals to authenticated;
create function public.make_admin(target uuid) returns void language sql security definer
  as $$ select 1 $$;                                                                          -- definer_function + mutable_search_path
grant execute on function public.make_admin(uuid) to anon;
create table public.private_stuff (id uuid primary key);                                     -- unreachable: never reported
revoke all on public.private_stuff from anon, authenticated;
