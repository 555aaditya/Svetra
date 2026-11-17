-- Rename note.body to note.content in place. RENAME COLUMN keeps every row's
-- text; the table's RLS, force RLS, grants and owner-only policies are
-- untouched (none of them reference the column).

alter table public.note rename column body to content;
