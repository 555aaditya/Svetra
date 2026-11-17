create table public.team (id uuid primary key default gen_random_uuid(), name text not null);
create table public.team_member (team uuid not null references public.team (id) on delete cascade, member uuid not null references auth.users (id) on delete cascade, primary key (team, member));
create table public.task (id uuid primary key default gen_random_uuid(), team uuid not null references public.team (id) on delete cascade, author uuid not null references auth.users (id), title text not null);
create function public.is_member(t uuid) returns boolean language sql stable security definer set search_path = '' as $$
  select exists (select 1 from public.team_member where team = t and member = (select auth.uid())) $$;
create function public.start_team(team_name text) returns uuid language plpgsql security definer set search_path = '' as $$
declare t uuid;
begin
  insert into public.team (name) values (team_name) returning id into t;
  insert into public.team_member (team, member) values (t, (select auth.uid()));
  return t;
end $$;
create function public.add_member(team_id uuid, person uuid) returns void language plpgsql security definer set search_path = '' as $$
begin
  if not public.is_member(team_id) then raise exception 'not a member' using errcode = '42501'; end if;
  insert into public.team_member (team, member) values (team_id, person) on conflict do nothing;
end $$;
