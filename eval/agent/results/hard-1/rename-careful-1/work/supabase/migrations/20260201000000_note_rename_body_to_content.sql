-- Rename note.body to note.content.
-- RENAME COLUMN keeps every existing row's text, and the table's row level
-- security, grants and owner-only policies stay in place unchanged.

alter table public.note rename column body to content;
