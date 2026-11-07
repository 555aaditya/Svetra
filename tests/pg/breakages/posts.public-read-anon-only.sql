-- The bug found on 2026-11-04 (PR #21): a public rule granted to logged-out visitors only, so
-- signed-in people couldn't see each other's public posts.
alter policy public_read on public.post to anon;
