-- Team task app: teams, membership, tasks, and all access rules.

create table public.team (
  id   uuid primary key default gen_random_uuid(),
  name text not null
);

create table public.team_member (
  team   uuid not null references public.team (id) on delete cascade,
  person uuid not null references auth.users (id) on delete cascade,
  primary key (team, person)
);
create index team_member_person_idx on public.team_member (person);

create table public.task (
  id     uuid primary key default gen_random_uuid(),
  team   uuid not null references public.team (id) on delete cascade,
  author uuid not null references auth.users (id) on delete cascade,
  title  text not null
);
create index task_team_idx on public.task (team);

alter table public.team        enable row level security;
alter table public.team_member enable row level security;
alter table public.task        enable row level security;
alter table public.team        force row level security;
alter table public.team_member force row level security;
alter table public.task        force row level security;

-- Membership check lives in a non-exposed schema and bypasses RLS, so policies
-- on team_member can use it without recursing into themselves.
create schema if not exists private;
revoke all on schema private from public;
grant usage on schema private to authenticated;

create function private.is_member(team_id uuid)
returns boolean
language sql
stable
security definer
set search_path = ''
as $$
  select exists (
    select 1 from public.team_member m
    where m.team = team_id and m.person = (select auth.uid())
  );
$$;
revoke all on function private.is_member(uuid) from public;
grant execute on function private.is_member(uuid) to authenticated;

-- Table privileges: nothing for anon; narrow grants for signed-in users.
-- Teams and memberships are only written through the functions below.
revoke all on public.team, public.team_member, public.task from public, anon, authenticated;
grant select on public.team        to authenticated;
grant select on public.team_member to authenticated;
grant select, delete on public.task to authenticated;
grant insert (team, author, title) on public.task to authenticated;
grant update (title)               on public.task to authenticated;  -- team/author are immutable

create policy team_select on public.team
  for select to authenticated
  using (private.is_member(id));

create policy member_select on public.team_member
  for select to authenticated
  using (private.is_member(team));

create policy task_select on public.task
  for select to authenticated
  using (private.is_member(team));

create policy task_insert on public.task
  for insert to authenticated
  with check (author = (select auth.uid()) and private.is_member(team));

create policy task_update on public.task
  for update to authenticated
  using (author = (select auth.uid()) and private.is_member(team))
  with check (author = (select auth.uid()) and private.is_member(team));

create policy task_delete on public.task
  for delete to authenticated
  using (author = (select auth.uid()) and private.is_member(team));

create function public.start_team(team_name text)
returns uuid
language plpgsql
security definer
set search_path = ''
as $$
declare
  uid uuid := (select auth.uid());
  new_id uuid;
begin
  if uid is null then
    raise exception 'not authenticated' using errcode = '28000';
  end if;
  if team_name is null or btrim(team_name) = '' then
    raise exception 'team name required' using errcode = '22023';
  end if;
  insert into public.team (name) values (team_name) returning id into new_id;
  insert into public.team_member (team, person) values (new_id, uid);
  return new_id;
end;
$$;

create function public.add_member(team_id uuid, person uuid)
returns void
language plpgsql
security definer
set search_path = ''
as $$
declare
  uid uuid := (select auth.uid());
begin
  if uid is null then
    raise exception 'not authenticated' using errcode = '28000';
  end if;
  if not private.is_member(team_id) then
    raise exception 'not a member of this team' using errcode = '42501';
  end if;
  insert into public.team_member (team, person)
  values (team_id, person)
  on conflict do nothing;
end;
$$;

revoke all on function public.start_team(text)      from public, anon;
revoke all on function public.add_member(uuid, uuid) from public, anon;
grant execute on function public.start_team(text)      to authenticated;
grant execute on function public.add_member(uuid, uuid) to authenticated;
