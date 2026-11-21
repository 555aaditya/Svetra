-- Tables
create table "public"."board" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "name" text not null
);

create table "public"."thread" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "board" uuid not null,
  "title" text not null
);

create table "public"."post" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "thread" uuid not null,
  "reply_to" uuid,
  "body" text
);

create table "public"."reaction" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "post" uuid not null
);

create table "public"."tag" (
  "id" uuid primary key default gen_random_uuid(),
  "owner" uuid not null,
  "post" uuid,
  "label" text
);

-- Foreign keys
alter table "public"."board" add constraint "board_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."thread" add constraint "thread_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."thread" add constraint "thread_board_fkey" foreign key ("board") references "public"."board" ("id") on delete cascade;
alter table "public"."post" add constraint "post_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."post" add constraint "post_thread_fkey" foreign key ("thread") references "public"."thread" ("id");
alter table "public"."post" add constraint "post_reply_to_fkey" foreign key ("reply_to") references "public"."post" ("id") on delete set null;
alter table "public"."reaction" add constraint "reaction_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."reaction" add constraint "reaction_post_fkey" foreign key ("post") references "public"."post" ("id") on delete cascade;
alter table "public"."tag" add constraint "tag_owner_fkey" foreign key ("owner") references "auth"."users" ("id");
alter table "public"."tag" add constraint "tag_post_fkey" foreign key ("post") references "public"."post" ("id") on delete set null;

-- Indexes
create index "board_owner_idx" on "public"."board" ("owner");
create index "thread_owner_idx" on "public"."thread" ("owner");
create index "thread_board_idx" on "public"."thread" ("board");
create index "post_owner_idx" on "public"."post" ("owner");
create index "post_thread_idx" on "public"."post" ("thread");
create index "post_reply_to_idx" on "public"."post" ("reply_to");
create index "reaction_owner_idx" on "public"."reaction" ("owner");
create index "reaction_post_idx" on "public"."reaction" ("post");
create index "tag_owner_idx" on "public"."tag" ("owner");
create index "tag_post_idx" on "public"."tag" ("post");

-- Access
alter table "public"."board" enable row level security;
alter table "public"."thread" enable row level security;
alter table "public"."post" enable row level security;
alter table "public"."reaction" enable row level security;
alter table "public"."tag" enable row level security;

-- Policies
create policy "boards_read" on "public"."board"
  for select to authenticated
  using (true);

create policy "boards_own_insert" on "public"."board"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

create policy "boards_own_update" on "public"."board"
  for update to authenticated
  using ("owner" = (select auth.uid()))
  with check ("owner" = (select auth.uid()));

create policy "boards_own_delete" on "public"."board"
  for delete to authenticated
  using ("owner" = (select auth.uid()));

create policy "threads_read" on "public"."thread"
  for select to authenticated
  using (true);

create policy "threads_own_insert" on "public"."thread"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

create policy "threads_own_update" on "public"."thread"
  for update to authenticated
  using ("owner" = (select auth.uid()))
  with check ("owner" = (select auth.uid()));

create policy "threads_own_delete" on "public"."thread"
  for delete to authenticated
  using ("owner" = (select auth.uid()));

create policy "posts_read" on "public"."post"
  for select to authenticated
  using (true);

create policy "posts_own_insert" on "public"."post"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

create policy "posts_own_update" on "public"."post"
  for update to authenticated
  using ("owner" = (select auth.uid()))
  with check ("owner" = (select auth.uid()));

create policy "posts_own_delete" on "public"."post"
  for delete to authenticated
  using ("owner" = (select auth.uid()));

create policy "reactions_read" on "public"."reaction"
  for select to authenticated
  using (true);

create policy "reactions_own_insert" on "public"."reaction"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

create policy "reactions_own_update" on "public"."reaction"
  for update to authenticated
  using ("owner" = (select auth.uid()))
  with check ("owner" = (select auth.uid()));

create policy "reactions_own_delete" on "public"."reaction"
  for delete to authenticated
  using ("owner" = (select auth.uid()));

create policy "tags_read" on "public"."tag"
  for select to authenticated
  using (true);

create policy "tags_own_insert" on "public"."tag"
  for insert to authenticated
  with check ("owner" = (select auth.uid()));

create policy "tags_own_update" on "public"."tag"
  for update to authenticated
  using ("owner" = (select auth.uid()))
  with check ("owner" = (select auth.uid()));

create policy "tags_own_delete" on "public"."tag"
  for delete to authenticated
  using ("owner" = (select auth.uid()));
