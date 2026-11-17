Build the backend for a private notes app on Supabase (PostgreSQL). Each person can create notes, and sees only their own notes. Only a note's owner can change or delete it.

Data contract, which the tests rely on: a table `note` with columns `id` (uuid, primary key, generated), `owner` (uuid, the signed-in person who created it, references auth.users) and `body` (text, required).

Put the final database schema, including everything that decides who can see and change data, in `supabase/migrations/`. Don't build a user interface.
