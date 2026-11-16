create table public.note (id uuid primary key default gen_random_uuid(), owner uuid not null references auth.users (id), body text not null);
