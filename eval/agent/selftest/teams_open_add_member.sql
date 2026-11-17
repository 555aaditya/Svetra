create table public.team (id uuid primary key default gen_random_uuid(), name text not null);
create table public.team_member (team uuid not null references public.team (id) on delete cascade, member uuid not null references auth.users (id) on delete cascade, primary key (team, member));
create table public.task (id uuid primary key default gen_random_uuid(), team uuid not null references public.team (id) on delete cascade, author uuid not null references auth.users (id), title text not null);
create function public.is_member(t uuid) returns boolean language sql stable security definer set search_path = '' as $$
  select exists (select 1 from public.team_member where team = t and member = (select auth.uid())) $$;
alter table public.team enable row level security;
alter table public.team_member enable row level security;
alter table public.task enable row level security;
create policy members_see_team on public.team for select to authenticated using (public.is_member(id));
create policy members_see_tasks on public.task for select to authenticated using (public.is_member(team));
create policy members_add_tasks on public.task for insert to authenticated with check (public.is_member(team) and author = (select auth.uid()));
create policy authors_change_tasks on public.task for update to authenticated using (author = (select auth.uid()) and public.is_member(team)) with check (author = (select auth.uid()) and public.is_member(team));
create policy authors_delete_tasks on public.task for delete to authenticated using (author = (select auth.uid()) and public.is_member(team));
create function public.start_team(team_name text) returns uuid language plpgsql security definer set search_path = '' as $$
declare t uuid;
begin
  if (select auth.uid()) is null then raise exception 'sign in first' using errcode = '42501'; end if;
  insert into public.team (name) values (team_name) returning id into t;
  insert into public.team_member (team, member) values (t, (select auth.uid()));
  return t;
end $$;
-- The mistake: anyone signed in can add anyone to any team.
create function public.add_member(team_id uuid, person uuid) returns void language plpgsql security definer set search_path = '' as $$
begin
  insert into public.team_member (team, member) values (team_id, person) on conflict do nothing;
end $$;
revoke execute on function public.start_team(text), public.add_member(uuid, uuid), public.is_member(uuid) from public, anon;
grant execute on function public.start_team(text), public.add_member(uuid, uuid), public.is_member(uuid) to authenticated;
