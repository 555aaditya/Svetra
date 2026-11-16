-- Blog posts: public posts are readable by everyone, private posts only by
-- their author, and only the author can write.

create table public.post (
  id        uuid primary key default gen_random_uuid(),
  author    uuid not null references auth.users (id) on delete cascade,
  title     text not null,
  is_public boolean not null default false
);

create index post_author_idx on public.post (author);

alter table public.post enable row level security;
alter table public.post force row level security;

-- Supabase grants broad default privileges to anon/authenticated; reset them
-- so access is exactly what is stated here (RLS then narrows rows further).
revoke all on table public.post from anon, authenticated;
grant select on table public.post to anon;
grant select, insert, update, delete on table public.post to authenticated;

-- Read: anyone (signed in or not) sees public posts.
create policy "public posts are readable by anyone"
  on public.post for select
  to anon, authenticated
  using (is_public);

-- Read: a signed-in person also sees their own private posts.
create policy "authors read their own posts"
  on public.post for select
  to authenticated
  using (author = (select auth.uid()));

-- Create: only under your own name.
create policy "authors create their own posts"
  on public.post for insert
  to authenticated
  with check (author = (select auth.uid()));

-- Change: only your own posts, and they cannot be reassigned to someone else.
create policy "authors update their own posts"
  on public.post for update
  to authenticated
  using (author = (select auth.uid()))
  with check (author = (select auth.uid()));

-- Delete: only your own posts.
create policy "authors delete their own posts"
  on public.post for delete
  to authenticated
  using (author = (select auth.uid()));
