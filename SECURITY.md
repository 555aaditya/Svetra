# Security policy

## Supported versions

Security fixes go into the latest release only. Upgrade before reporting, in case it's already fixed.

## What counts as a vulnerability

Svetra generates access policies, so its own bugs can weaken the security of the apps built with it. All of these are security issues:

- Generated row-level security that grants more access than the `svetra.yaml` declares.
- `svetra validate` accepting a file it should reject under a security rule, such as SVT001 (an exposed table with no policy).
- `svetra check` missing a table with no row-level security, or a policy that differs from the graph.
- Svetra executing or loading anything from a `svetra.yaml` other than data.

## Reporting

**Don't open a public issue.** Use **Report a vulnerability** on the repository's Security tab. If that isn't available, open an issue that only asks for a private contact, with no technical details.

Include:
- the Svetra version
- a minimal `svetra.yaml`
- the command you ran
- what Svetra produced, and what it should have produced

Remove any real credentials, data, or project names you don't want public.

The maintainer will acknowledge the report, confirm the issue, and agree a disclosure date with you.

## Boundaries

- Svetra checks the graph and the code it generates.
- In version 0.1 it does **not** analyse hand-written hole bodies. They're limited by their generated signatures, but code in a hole can still reach the database directly.
- Svetra never contacts a deployed application. `svetra check` reads only the database or files you point it at.
