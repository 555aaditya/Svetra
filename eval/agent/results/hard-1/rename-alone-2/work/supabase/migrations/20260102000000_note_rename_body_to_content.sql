-- Rename note.body to note.content in place. RENAME COLUMN keeps every
-- existing row's text; policies, grants, RLS and the owner-lock trigger do not
-- reference this column, so they are unaffected.

alter table public.note rename column body to content;
