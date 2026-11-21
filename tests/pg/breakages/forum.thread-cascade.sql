-- A hand edit that turns the default into a cascade: the posts of a thread no longer protect it.
-- The tests expect those deletes to be refused, because svetra.yaml says `restrict`.
alter table public.post drop constraint post_thread_fkey;
alter table public.post add constraint post_thread_fkey foreign key (thread) references public.thread (id) on delete cascade;
