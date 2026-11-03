-- Tables
create table "public"."audit_log" (
  "id" uuid primary key default gen_random_uuid(),
  "note" text
);

create table "public"."post" (
  "id" uuid primary key default gen_random_uuid(),
  "author" uuid not null,
  "body" text
);

-- Foreign keys
alter table "public"."post" add constraint "post_author_fkey" foreign key ("author") references "auth"."users" ("id");

-- Indexes
create index "post_author_idx" on "public"."post" ("author");

-- Access
alter table "public"."audit_log" enable row level security;
revoke all on table "public"."audit_log" from anon, authenticated;
alter table "public"."post" enable row level security;

-- Policies
create policy "posts_by_author_select" on "public"."post"
  for select to authenticated
  using ("author" = (select auth.uid()));

create policy "posts_by_author_insert" on "public"."post"
  for insert to authenticated
  with check ("author" = (select auth.uid()));

create policy "posts_by_author_update" on "public"."post"
  for update to authenticated
  using ("author" = (select auth.uid()))
  with check ("author" = (select auth.uid()));

create policy "posts_by_author_delete" on "public"."post"
  for delete to authenticated
  using ("author" = (select auth.uid()));

create policy "public_read" on "public"."post"
  for select to anon
  using (true);
