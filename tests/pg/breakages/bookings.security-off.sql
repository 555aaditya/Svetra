-- Row-level security turned off on one table: every check that expects "can't" on it must fail.
alter table public.booking disable row level security;
