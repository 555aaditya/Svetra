# Repository guidelines

These notes are for anyone, human or agent, changing Svetra itself. Instructions for agents that *use* Svetra to build apps will ship separately as the Svetra skill.

Svetra is a deterministic compiler for `svetra.yaml`, a small typed system graph of entities, constraints, policies and holes. The spec is [`docs/format.md`](docs/format.md). **The spec is the authority:** a behaviour change updates the spec in the same PR.

## Priorities

1. **Agent tokens first, then wall time, then CPU.** Output must be compact, and every diagnostic must say what to do next.
2. **No speculative complexity.** Build what the current milestone needs. No new abstractions, options or dependencies "for later".
3. **Fix root causes.** When a fix supersedes old code, delete the old code.
4. **Deterministic:** the same input always gives the same output. No clocks, randomness or hash-map iteration order in anything that is emitted.

## Layout

Dependencies flow one way: `yaml` → `model` → `load` → `expr`/`typeck` → `check` → `report` → `inspect` → `infer` → `verify` → `main`.

| Path | Owns |
|---|---|
| `src/migrations.rs`, `src/standin/` | `svetra check --migrations`: builds migrations in a throwaway database (Docker, or `--scratch`); also the throwaway Supabase copy (`Copy`, with pgTAP) for `infer --check-draft`. `standin/` is the Supabase stand-in (base and extra SQL), compiled in and shared with `scripts/` and `eval/` |
| `tests/pg/infer/shop_app.sql` | the `infer` test shop. Its snapshot (`tests/fixtures/infer/shop_app.json`) is taken on PostgreSQL 16 after `src/standin/base.sql` **and** `extra.sql` (`auth.role()` lives in `extra.sql`); regenerate the snapshot and the draft golden together |
| `src/yaml.rs` | The YAML tree with line and column positions. The only module that touches `saphyr` |
| `src/model.rs` | The typed model of the format. Plain data, no logic |
| `src/diag.rs` | `Code`: every diagnostic's id, severity, category and remediation action, in one table |
| `src/load/` | YAML to model, plus structural diagnostics (SVT019, 020, 023, 025, 028, 029). Also owns the allowed-key constants |
| `src/expr.rs` | The expression lexer and parser |
| `src/typeck.rs` | Expression typing (SVT003) |
| `src/check.rs` | Whole-document rules: names, duplicates, references, access, money, indexes |
| `src/report.rs` | `validate()`, plus human and JSON output |
| `src/compile/` | `svetra compile`: names and quoting (`names.rs`), expressions to SQL (`expr.rs`), the SQL schema (`sql.rs`), TypeScript and Zod (`ts.rs`), access tests with their own rule evaluator (`access_tests.rs`), sign-in settings edited into `supabase/config.toml` (`config.rs`), plus the entry point with header and revision (`mod.rs`) |
| `src/change.rs` | `svetra diff` and `plan`: compares two designs and classifies every change, with access compared through `svetra view`'s model (docs/change.md) |
| `src/inspect.rs` | `svetra check`: the read-only catalog query and the findings on an existing database (docs/check.md) |
| `src/infer.rs` | `svetra infer`: a catalog snapshot to a draft `svetra.yaml`: names, the type mapping, defaults, check constraints (docs/infer.md). Its query is `inspect::infer_query` |
| `src/verify.rs` | `svetra infer --check-draft`: reads a draft and the output of its generated access tests run on a copy of the original database (`migrations::Copy`), and reports each disagreement by entity (docs/infer.md) |
| `src/doctor/` | `svetra doctor`: the project-folder checks plus `check`'s findings under `data` (via `inspect::run`), read-only and with no network: the walk and git lookups (`files.rs`), reading a JWT's role (`jwt.rs`), leaked keys and committed `.env` files (`keys.rs`), the generated-files drift check on `project::compile` (`generated.rs`), `schema_not_in_git` (`backups.rs`), the auth-setting checks with their small `config.toml` reader (`auth.rs`), `public_bucket` and the report (`mod.rs`). Its database read is `inspect::doctor_query`, `check`'s query plus a `buckets` key; `check`'s own query is unchanged. The `doctor` tool of `svetra mcp` calls `run` Never prints a secret's value (docs/doctor.md). Tests build their projects at run time, so no key-shaped string is committed |
| `src/main.rs` | The CLI |
| `schema/svetra.schema.json` | The JSON Schema for editors. Hand-written; `tests/schema.rs` fails if it drifts from the loader |
| `tests/` | Integration tests. `tests/fixtures/` holds example `svetra.yaml` files (and real hole bodies in `tests/fixtures/holes/<fixture>/`), and `tests/golden/` holds their expected compiler output, compared byte for byte |
| `site/` | The website: Vite, React, Tailwind v4 and BoardUI's free components (MIT, see `site/LICENSE-BOARDUI`). Shows only real output, imported from `tests/` at build time. `npm run build` in `site/`; deployed from `main` by `pages.yml` |
| `eval/agent/` | The agent evaluation pilot: headless Claude Code on fixed prompts, then deterministic checks of what it built on a real PostgreSQL. Not shipped; see its README for what it can and cannot show |
| `eval/leakstudy/` | The leak study harness: `svetra check` on the migrations of open-source Supabase apps, in a sealed throwaway PostgreSQL. Code only; the data (repo names, findings) is git-ignored and never published. Not shipped |
| `npm/` | The npm wrapper: downloads, verifies and caches the release binary on first run |
| `tests/pg/`, `scripts/pg-check.sh` | Compiles the fixtures, applies the SQL to PostgreSQL 16 with a Supabase stand-in, and asserts behaviour: who sees which rows, and what's rejected. Then runs the generated access tests with pgTAP, and each deliberate breakage in `tests/pg/breakages/` (a `.sql` change plus the exact `.expected` failures), on its own copy of the database |
| `scripts/supabase-check.sh` | The generated access tests and every breakage again, on a real local Supabase started by the pinned Supabase CLI, run as Supabase's own `postgres` role. Catches differences between Supabase and the stand-in |
| `scripts/mutate.py` | The mutation study of the generated access tests: breaks a deployed database one way at a time and counts the mutants the tests catch. Runs on the PostgreSQL stand-in, or with `--supabase` on a real local Supabase |

## Commands

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo +1.88 check --all-targets      # the minimum supported Rust version
cargo run -- validate tests/fixtures/finledger.yaml [--json]
scripts/pg-check.sh                  # needs Docker: real-Postgres behaviour checks
scripts/supabase-check.sh            # needs Docker and Node: the access tests on a real local Supabase
scripts/mutate.py [--supabase]       # needs Docker (and Node with --supabase): the mutation study
```

CI runs all of these. A PR is ready when they pass locally.

## Rules for changes

- **Diagnostic codes are stable.** Never reuse or renumber a code. A new rule needs all four of these:
  1. a row in `diag.rs`
  2. a row in the spec's Diagnostics table
  3. a test that triggers it
  4. a message that says what is wrong **and** a hint that says how to fix it
- **Every diagnostic has a position.** Report problems as diagnostics, never by panicking, and keep collecting after the first one.
- **Keep the JSON Schema in sync.** A new key or type goes into the loader's constants and the schema together. `tests/schema.rs` enforces this.
- **Dependencies:** at most one runtime crate per real need. Dev-only crates are fine for tests.
- **No `unsafe`, and no `unwrap` or `expect` outside tests.**
- **Generated output must be readable and stable.** It's reviewed in diffs. A change to compiler output updates the golden files, and must still pass `scripts/pg-check.sh`.

## Tests

- Name integration tests after what they check (`tests/check.rs`, `tests/expressions.rs`).
- **One test per diagnostic code**, asserting the code and, where it matters, the position.
- **CLI tests run the real binary** (`CARGO_BIN_EXE_svetra`) and check exit codes and output.
- Use fixtures for whole documents. Don't mock the loader.

## Commits and pull requests

- **Commits:** one logical change each, kept small, with a simple imperative subject under 72 characters, for example `Add validate command`.
- **PRs target `develop`, with one topic per PR.** They're squash- or rebase-merged by the maintainer.
- **Describe the change:**
  - what changed and why
  - the commands you ran
  - any change to the format, diagnostics or JSON output
- **No AI co-author trailers.**
- **Never commit** `target/`, credentials, real user data, or local agent folders (`.codex/`, `.devin/`, `.factory/`).
