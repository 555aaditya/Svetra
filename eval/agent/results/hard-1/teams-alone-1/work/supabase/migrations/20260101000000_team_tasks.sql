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
  author uuid not null references auth.users (id),
  title  text not null
);

create index task_team_idx on public.task (team);

alter table public.team        enable row level security;
alter table public.team_member enable row level security;
alter table public.task        enable row level security;
alter table public.team        force row level security;
alter table public.team_member force row level security;
alter table public.task        force row level security;

-- Membership check. SECURITY DEFINER so policies on team_member can use it
-- without recursing into their own RLS.
create function public.is_team_member(team_id uuid)
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

-- Table privileges: signed-in users only, and only what the rules allow.
revoke all on public.team, public.team_member, public.task from anon, authenticated, public;
grant select on public.team, public.team_member to authenticated;
grant select, insert, update, delete on public.task to authenticated;

-- team: members read. Creation only through start_team().
create policy team_select on public.team
  for select to authenticated
  using (public.is_team_member(id));

-- team_member: members see who is in their teams. Changes only through functions.
create policy team_member_select on public.team_member
  for select to authenticated
  using (public.is_team_member(team));

-- task: members read and add; only the author changes or deletes.
create policy task_select on public.task
  for select to authenticated
  using (public.is_team_member(team));

create policy task_insert on public.task
  for insert to authenticated
  with check (author = (select auth.uid()) and public.is_team_member(team));

create policy task_update on public.task
  for update to authenticated
  using (author = (select auth.uid()) and public.is_team_member(team))
  with check (author = (select auth.uid()) and public.is_team_member(team));

create policy task_delete on public.task
  for delete to authenticated
  using (author = (select auth.uid()) and public.is_team_member(team));

-- Keep a task in the team it was created in.
create function public.task_team_immutable()
returns trigger
language plpgsql
set search_path = ''
as $$
begin
  if new.team is distinct from old.team then
    raise exception 'a task cannot be moved to another team';
  end if;
  return new;
end;
$$;

create trigger task_team_immutable
  before update on public.task
  for each row execute function public.task_team_immutable();

create function public.start_team(team_name text)
returns uuid
language plpgsql
security definer
set search_path = ''
as $$
declare
  me uuid := (select auth.uid());
  new_id uuid;
begin
  if me is null then
    raise exception 'not signed in' using errcode = '28000';
  end if;
  insert into public.team (name) values (team_name) returning id into new_id;
  insert into public.team_member (team, person) values (new_id, me);
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
  me uuid := (select auth.uid());
begin
  if me is null then
    raise exception 'not signed in' using errcode = '28000';
  end if;
  if not exists (
    select 1 from public.team_member m where m.team = team_id and m.person = me
  ) then
    raise exception 'not a member of this team' using errcode = '42501';
  end if;
  insert into public.team_member (team, person) values (team_id, person)
  on conflict do nothing;
end;
$$;

revoke all on function public.is_team_member(uuid) from public, anon;
revoke all on function public.start_team(text) from public, anon;
revoke all on function public.add_member(uuid, uuid) from public, anon;
grant execute on function public.is_team_member(uuid) to authenticated;
grant execute on function public.start_team(text) to authenticated;
grant execute on function public.add_member(uuid, uuid) to authenticated;
