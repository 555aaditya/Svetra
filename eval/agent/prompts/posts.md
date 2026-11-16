Build the backend for a small blog on Supabase (PostgreSQL). Anyone, even someone who isn't signed in, can read a post that its author has made public. A signed-in person can also read their own private posts, and nobody else's. Only a post's author can create, change or delete it, and a person can only create posts under their own name.

Data contract, which the tests rely on: a table `post` with columns `id` (uuid, primary key, generated), `author` (uuid, the signed-in person who wrote it, references auth.users), `title` (text, required) and `is_public` (boolean, required, false by default).

Put the final database schema, including everything that decides who can see and change data, in `supabase/migrations/`. Don't build a user interface.
