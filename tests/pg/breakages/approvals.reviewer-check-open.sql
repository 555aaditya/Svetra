-- The reviewer's rule lost its `check`: they can now put a document into any state.
drop policy "reviewers_decide" on "public"."doc";
create policy "reviewers_decide" on "public"."doc" for update to authenticated
  using ("reviewer" = (select auth.uid()) and "status" = 'submitted')
  with check ("reviewer" = (select auth.uid()));
