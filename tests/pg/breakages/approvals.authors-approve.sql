-- The author's rule now lets a draft go straight to approved.
drop policy "authors_edit" on "public"."doc";
create policy "authors_edit" on "public"."doc" for update to authenticated
  using ("author" = (select auth.uid()) and "status" = 'draft')
  with check ("author" = (select auth.uid()) and "status" in ('draft', 'submitted', 'approved'));
