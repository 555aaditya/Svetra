-- Document approvals. Table, constraints and row-level security come from
-- svetra.yaml (same as supabase/schemas/svetra.sql); the trigger at the end is
-- hand-written because policies cannot compare the old row with the new one.

create table "public"."doc" (
  "id" uuid primary key default gen_random_uuid(),
  "author" uuid not null,
  "reviewer" uuid not null,
  "title" text not null,
  "status" text not null default 'draft',
  constraint "doc_status_values" check ("status" in ('draft', 'submitted', 'approved', 'rejected'))
);

alter table "public"."doc" add constraint "doc_author_fkey" foreign key ("author") references "auth"."users" ("id");
alter table "public"."doc" add constraint "doc_reviewer_fkey" foreign key ("reviewer") references "auth"."users" ("id");

create index "doc_author_idx" on "public"."doc" ("author");
create index "doc_reviewer_idx" on "public"."doc" ("reviewer");

alter table "public"."doc" enable row level security;

create policy "author_sees_own" on "public"."doc"
  for select to authenticated
  using ("author" = (select auth.uid()));

create policy "reviewer_sees_submitted" on "public"."doc"
  for select to authenticated
  using ("reviewer" = (select auth.uid()) and "status" <> 'draft');

create policy "author_creates_draft" on "public"."doc"
  for insert to authenticated
  with check ("author" = (select auth.uid()) and "reviewer" <> (select auth.uid()) and "status" = 'draft');

create policy "author_edits_or_submits_draft" on "public"."doc"
  for update to authenticated
  using ("author" = (select auth.uid()) and "status" = 'draft')
  with check ("author" = (select auth.uid()) and "reviewer" <> (select auth.uid()) and "status" in ('draft', 'submitted'));

create policy "author_deletes_draft" on "public"."doc"
  for delete to authenticated
  using ("author" = (select auth.uid()) and "status" = 'draft');

create policy "reviewer_decides" on "public"."doc"
  for update to authenticated
  using ("reviewer" = (select auth.uid()) and "status" = 'submitted' and "author" <> (select auth.uid()))
  with check ("reviewer" = (select auth.uid()) and "status" in ('approved', 'rejected'));

-- Column rules: id, author and reviewer never change, and the title can only
-- change while the document is a draft (so a reviewer cannot retitle while
-- deciding). Skipped when there is no signed-in user (database owner, setup).
create function "public"."doc_guard_columns"() returns trigger
  language plpgsql
  security invoker
  set search_path = ''
as $$
begin
  if (select auth.uid()) is null then
    return new;
  end if;
  if new."id" is distinct from old."id"
     or new."author" is distinct from old."author"
     or new."reviewer" is distinct from old."reviewer" then
    raise exception 'id, author and reviewer of a document cannot change' using errcode = '42501';
  end if;
  if old."status" <> 'draft' and new."title" is distinct from old."title" then
    raise exception 'only a draft can be retitled' using errcode = '42501';
  end if;
  return new;
end;
$$;

create trigger "doc_guard_columns" before update on "public"."doc"
  for each row execute function "public"."doc_guard_columns"();
