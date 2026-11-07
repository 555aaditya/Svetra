---
name: svetra
description: Design and change a Supabase app through svetra.yaml, the one file that decides its data, who can see and change what, roles, sign-in, and where custom code goes. Use whenever the person wants to add or change tables, fields, access rules, roles or teams, sign-in or two-factor settings, or server-side logic in a project that has svetra.yaml, and to answer "who can see X?".
---

<!-- Written by `svetra init`, and replaced each time it runs. Edit svetra.yaml, not this file. -->

# Svetra

This project's data layer is designed in `svetra.yaml`. Svetra checks it, compiles it into Postgres with row-level security on every table, TypeScript and Zod types, sign-in settings and access tests, and explains it in plain English. **You design with the person; Svetra makes it safe; you build the rest.**

Use the `svetra` MCP tools (`validate`, `compile`, `explain`, `context`, `graph`). If they aren't available, run the same commands with `npx -y svetra <command>`; add `--json` to `validate`.

## The loop

1. **Design first.** Turn the request into a change to `svetra.yaml`: entities, fields, policies, roles, `auth`, and holes only for logic that a policy can't express. Ask at most three questions, each with a recommended default, and only when the answer changes the design.
2. **Validate after every edit.** Fix every error using its `remediation.hint` before doing anything else. Never move on with errors. Warnings and info are worth reading: they usually point at a real gap.
3. **Compile.** Never edit what Svetra generates:
   - `supabase/schemas/svetra.sql`
   - `src/svetra.gen.ts`
   - `supabase/tests/svetra.test.sql`
   - the sign-in keys Svetra sets in `supabase/config.toml`

   Change `svetra.yaml` and compile again instead.
4. **Write only hole bodies,** in `supabase/holes/<name>.sql` (PL/pgSQL). Inputs are `p_<input>`, and `p_caller` is the signed-in person's id. A hole can touch only the entities in its `reads` and `writes`, and its `when` already decides who may call it, so don't repeat that check in the body.
5. **Explain.** Call `explain` and tell the person what changed in their words: who can see and change what, and "Before going live" when there is one. Never paste SQL at them.
6. **Prove it.** If a local Supabase is running, `supabase test db` runs the access tests as real users. Report failures exactly as they are.

## Rules

- **Never** use the service role key in app code, and never turn row-level security off.
- **Never** weaken a rule to make a test pass. A failing access test means the design and the database disagree; find out which is wrong.
- **Prefer a policy over a hole, and a hole over app code,** for anything about who can see or change data.
- **App code calls holes** through the typed functions in `src/svetra.gen.ts`, such as `transfer(supabase, input)`.
- **Read the design with `context`**, not by opening the generated files.
- **When asked "who can see X?",** answer from `explain`, not from memory.

## The format, briefly

```yaml
svetra: "0.1"
system: Bookings
auth:
  sign_in: [email, google]
roles:
  admin: { description: "Runs the business", mfa: true }
  staff: { description: "Works at one location", scope: Location }
entities:
  Location:
    fields:
      id:   { type: uuid, primary: true }
      name: { type: text, required: true }
  Booking:
    fields:
      id:       { type: uuid, primary: true }
      customer: { type: user, required: true }
      location: { type: ref, entity: Location, required: true }
policies:
  customers_see_own:  { on: Booking, allow: [select, insert], when: "customer == auth.user" }
  staff_manage_local: { on: Booking, allow: [select, update], when: "has_role('staff', location)" }
  admins_run_locations: { on: Location, allow: [select, insert, update], when: "has_role('admin')" }
  staff_see_location: { on: Location, allow: [select], when: "has_role('staff', id)" }
holes:
  cancel:
    kind: function
    input: { booking: Booking.id }
    writes: [Booking]
    when: "booking.customer == auth.user"
```

The full specification is at https://github.com/555aaditya/Svetra/tree/main/docs.
