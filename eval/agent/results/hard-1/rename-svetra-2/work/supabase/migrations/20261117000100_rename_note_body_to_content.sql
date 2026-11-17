-- Rename note.body to note.content. A rename keeps every value; no data is lost.
alter table "public"."note" rename column "body" to "content";
