-- A hand edit that drops `on delete set null` from a tag's post: a post that a tag points at can no
-- longer be deleted. The tests expect that delete to go through, because svetra.yaml says `set_null`.
alter table public.tag drop constraint tag_post_fkey;
alter table public.tag add constraint tag_post_fkey foreign key (post) references public.post (id);
