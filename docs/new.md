# `svetra new`, version 0.1

This document specifies `svetra new`: a starter `svetra.yaml` from a small set of templates, for someone who has no idea where to begin. The format is in [`format.md`](format.md) and the agent setup is in [`agents.md`](agents.md). When the code and this document disagree, that's a bug in one of them.

**Done when:** someone who has never seen Svetra runs one command, reads the file it wrote without help, and has a design that validates and compiles within five minutes.

## Command

```sh
svetra new <template> [DIR]
svetra new --list
```

- **`<template>`** is one of the names below. `DIR` defaults to the current folder, and is created if it doesn't exist.
- **`--list`** prints each template's name and a one-line description in plain English, one per line, in the order below, and writes nothing. It's the only option.
- **It writes `svetra.yaml` and nothing else.** No skill, no `.mcp.json`, no generated files. Those come from `svetra init` and `svetra compile`.
- **It never overwrites.** If `DIR/svetra.yaml` exists, it writes nothing, prints "svetra.yaml already exists here; `svetra new` never replaces your design. Pick another folder, or move the file first." and exits `2`.
- **An unknown template** exits `2` with "no template called 'x'", then the list of template names. With no template at all it does the same.
- **It checks its own work.** After writing, it validates the file with the same code as `svetra validate`. Every shipped template validates with no diagnostics, and a test enforces that, so this is a safeguard, not a feature: if it ever failed, the diagnostics are printed and the exit code is `1`.
- **It prints the next steps,** and doesn't run them:

  ```
  wrote svetra.yaml (notes: ...)
  Next:
    svetra explain      read the rules in plain English
    svetra compile      write the SQL, types and access tests
    svetra init         set up your coding agent, when you want it to take over
  ```

  The `cd DIR` line appears first when `DIR` isn't the current folder.
- **Deterministic:** the same template gives byte-identical output.

## How it relates to `init`

They are separate on purpose. `new` is about the design; `init` is about the agent.

- `new` does **not** run `init`'s agent setup. Someone who wants to read and edit the file by hand shouldn't get a `.claude/` folder and an MCP config they didn't ask for.
- `init` still writes its own small starter when there is no `svetra.yaml`, and still never overwrites one. So `svetra new bookings && svetra init` gives the bookings design plus agent setup, and `svetra init && svetra new ...` is refused (init already wrote a file).
- The `notes` template is `init`'s starter plus a few friendly comments and a `done` flag, so the two never disagree about what "the simplest app" is.

## Templates

Each is small (about 20 to 50 lines of design), valid, and commented in plain words: every policy has a comment saying who can do what, in the words `svetra explain` would use. Templates are product content, shipped inside the binary, and are not generated from the test fixtures; they may be based on them.

| Name | Description | What it shows |
|---|---|---|
| `notes` | A private to-do list: each person sees only their own items | A `user` field and one policy, `author == auth.user`. Email sign-in |
| `blog` | A blog: anyone can read posts, only the author can write them | `as: anyone` for public reading, and the split of read and write policies |
| `bookings` | Customers book a slot; staff run one location; admins run everything | Roles, one global (`admin`) and one scoped to a location (`staff`), and `has_role()` in a rule |
| `teams` | Teams of people who share tasks; only members see their team's tasks | A role scoped to a team, a `ref` between entities, and a hole for "start a team" |
| `approvals` | Write a document, send it to a reviewer who can approve or reject it and change nothing else | `check` for allowed state changes, `only` to lock the other fields, and a constraint that keeps author and reviewer different |

The names are lowercase, one word, and stable: renaming one is a breaking change to the command.

## Files written

| File | What | If it exists |
|---|---|---|
| `svetra.yaml` | The template, with comments | Refused, exit `2`, nothing written |

A template that declares a hole (`teams`) doesn't write the hole's body. `svetra compile` writes the stub, as for any design.

## Exit codes

`0` written; `1` the written file had diagnostics (a bug in a template); `2` usage: unknown template, a file already there, or an unwritable folder.
