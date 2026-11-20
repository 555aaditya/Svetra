-- A function the API can call, and a table nobody can reach.
create function public.make_admin(target uuid) returns void language sql security definer
  as $$ select 1 $$;                                                                          -- definer_function + mutable_search_path
grant execute on function public.make_admin(uuid) to anon;
create table public.private_stuff (id uuid primary key);                                     -- unreachable: never reported
revoke all on public.private_stuff from anon, authenticated;
