-- Rename note.body to note.content. A rename keeps every existing row's text,
-- the not-null constraint, and the table's RLS policies and grants untouched.

alter table public.note rename column body to content;
