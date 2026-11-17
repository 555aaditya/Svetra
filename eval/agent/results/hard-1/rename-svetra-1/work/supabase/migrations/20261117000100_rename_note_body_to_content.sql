-- Rename note.body to note.content. A column rename keeps every existing value.
alter table "public"."note" rename column "body" to "content";
