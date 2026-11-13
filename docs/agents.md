# Agents: the skill, the MCP server and `svetra init`, version 0.1

This document specifies how coding agents use Svetra: Claude Code, Cursor, Codex and anything else that speaks the Model Context Protocol (MCP). The format is in [`format.md`](format.md) and the outputs are in [`compiler.md`](compiler.md). When the code and this document disagree, that's a bug in one of them.

The model, from the project's vision: **you design the data with your agent, Svetra makes it safe, and the agent builds the rest.** The agent edits `svetra.yaml` and writes hole bodies. Svetra checks every edit, compiles everything else, and explains the result in plain English. The agent never writes SQL policies, grants or types by hand.

## `svetra init`

```sh
svetra init [DIR]
```

Sets a project up for agents. `DIR` defaults to the current folder. It writes:

| File | What | If it exists |
|---|---|---|
| `.claude/skills/svetra/SKILL.md` | The Svetra skill (below) | Replaced, so it stays current. It's generated and marked as such |
| `.mcp.json` | Adds a `svetra` server: `{"command": "npx", "args": ["-y", "svetra", "mcp"]}`. `-y` matters: agents start MCP servers without a terminal, so npx's install prompt would hang | The `svetra` entry is added or updated, and every other server and key is kept, in order. Invalid JSON is refused, not overwritten |
| `svetra.yaml` | A valid starting file: a `Note` that only its author can see and change, and an `auth` section with email sign-in | **Never overwritten** |

`init` prints what it wrote and the next step: "Open your agent and describe the app you want."

**Behind a company proxy** that inspects HTTPS, `npx` can't download the binary until `NODE_EXTRA_CA_CERTS` points at the proxy's root certificate. Set it before `init`, and in the terminal the agent is started from: the agent starts `svetra mcp` with its own environment, so an agent opened another way (for example from an editor) needs the variable too, or the server fails to start.

## The skill

`SKILL.md` teaches the agent one loop, plus the rules that keep it safe. Its content is fixed by this section:

1. **Design first.** Turn the person's request into `svetra.yaml`: entities, fields, policies, roles, `auth`, and holes only for logic that can't be a policy. Ask at most three questions, each with a recommended default.
2. **Validate after every edit,** with the `validate` tool, or `svetra validate --json`. Fix every error using its `remediation.hint` before going on. Don't move on with errors.
3. **Compile,** with the `compile` tool. Never edit the generated files (`supabase/schemas/svetra.sql`, `src/svetra.gen.ts`, `supabase/tests/svetra.test.sql`), or the keys Svetra owns in `supabase/config.toml`.
4. **Write only hole bodies,** in `supabase/holes/<name>.sql`. Use the `p_` inputs, touch only the declared entities, and never add a check that `when` already makes.
5. **Explain,** with the `explain` tool. Show the person the "Who can see and change what" section, and "Before going live" if there is one, in their words, not SQL.
6. **Prove it,** with `supabase test db` when a local Supabase is running. Report failures as they are, and never weaken a policy to make a test pass.

And these rules:
- **Never** use the service role key in app code, and never disable row-level security.
- **Prefer a policy over a hole,** and a hole over app code, for anything about who can see or change data.
- **Use `context`** to read the whole design compactly, instead of opening the generated files.
- **When the person asks "who can see X?",** call `explain` again and answer from its lines, even if it ran earlier in the conversation. Never answer from memory: the file may have changed.

## The MCP server

```sh
svetra mcp
```

An MCP server over stdio: JSON-RPC 2.0, one message per line. It supports `initialize`, `tools/list` and `tools/call`, and answers `ping`. Every tool takes a `file` (default `svetra.yaml`, relative to the server's working directory) and returns JSON text the agent can act on, never terminal formatting.

| Tool | Does | Returns |
|---|---|---|
| `validate` | `svetra validate --json` | The diagnostics JSON, exactly as the CLI prints it |
| `compile` | `svetra compile`, or `--check` with `check: true` | `{"written": [...], "notes": [...]}`, or `{"stale": [...]}` with `check`. On errors, the diagnostics |
| `explain` | `svetra explain` | `{"text": "..."}` |
| `context` | `svetra context` | `{"text": "..."}` |
| `graph` | `svetra graph` | `{"mermaid": "..."}` |

- **No editing tools:** agents already edit files. Svetra's job is to check and compile what they wrote, and keeping it read-and-compile only keeps the server small and safe.
- **Errors:** a tool that can't run, such as a missing file, returns an MCP tool error with one sentence saying what to do, never a stack trace.
- **Protocol version:** the server answers with the version the client asks for if it supports it, and its own otherwise. Version 0.1 supports `2025-06-18`.

## Not in version 0.1

- `svetra patch` and `svetra diff`, so the agent proposes a change as a reviewable graph diff before writing it: planned with safe change, M6.
- Tools that edit `svetra.yaml` directly.
- Setup files for other agents. Claude Code reads `SKILL.md` and the project's `.mcp.json`. Cursor (`.cursor/mcp.json` and rules) and Codex (its `config.toml` and `AGENTS.md`) can run `svetra mcp` today if added by hand; `init` writing their files comes later.
