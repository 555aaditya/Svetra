alter table public.note drop column body;
alter table public.note add column content text not null default '';
