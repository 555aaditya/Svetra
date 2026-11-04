# Repository guidelines

These notes are for anyone, human or agent, changing Svetra itself. Instructions for agents that *use* Svetra to build apps will ship separately as the Svetra skill.

Svetra is a deterministic compiler for `svetra.yaml`, a small typed system graph of entities, constraints, policies and holes. The spec is [`docs/format.md`](docs/format.md). **The spec is the authority:** a behaviour change updates the spec in the same PR.

## Priorities

1. **Agent tokens first, then wall time, then CPU.** Output must be compact, and every diagnostic must say what to do next.
2. **No speculative complexity.** Build what the current milestone needs. No new abstractions, options or dependencies "for later".
3. **Fix root causes.** When a fix supersedes old code, delete the old code.
4. **Deterministic:** the same input always gives the same output. No clocks, randomness or hash-map iteration order in anything that is emitted.

## Layout

Dependencies flow one way: `yaml` → `model` → `load` → `expr`/`typeck` → `check` → `report` → `main`.

| Path | Owns |
|---|---|
| `src/yaml.rs` | The YAML tree with line and column positions. The only module that touches `saphyr` |
| `src/model.rs` | The typed model of the format. Plain data, no logic |
| `src/diag.rs` | `Code`: every diagnostic's id, severity, category and remediation action, in one table |
| `src/load/` | YAML to model, plus structural diagnostics (SVT019, 020, 023, 025, 028, 029). Also owns the allowed-key constants |
| `src/expr.rs` | The expression lexer and parser |
| `src/typeck.rs` | Expression typing (SVT003) |
| `src/check.rs` | Whole-document rules: names, duplicates, references, access, money, indexes |
| `src/report.rs` | `validate()`, plus human and JSON output |
| `src/compile/` | `svetra compile`: names and quoting (`names.rs`), expressions to SQL (`expr.rs`), the SQL schema (`sql.rs`), TypeScript and Zod (`ts.rs`), access tests with their own rule evaluator (`access_tests.rs`), plus the entry point with header and revision (`mod.rs`) |
| `src/main.rs` | The CLI |
| `schema/svetra.schema.json` | The JSON Schema for editors. Hand-written; `tests/schema.rs` fails if it drifts from the loader |
| `tests/` | Integration tests. `tests/fixtures/` holds example `svetra.yaml` files, and `tests/golden/` holds their expected compiler output, compared byte for byte |
| `tests/pg/`, `scripts/pg-check.sh` | Compiles the fixtures, applies the SQL to PostgreSQL 16 with a Supabase stand-in, and asserts behaviour: who sees which rows, and what's rejected. Then runs the generated access tests with pgTAP, and checks that a leaking policy makes them fail |

## Commands

```sh
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo +1.88 check --all-targets      # the minimum supported Rust version
cargo run -- validate tests/fixtures/finledger.yaml [--json]
scripts/pg-check.sh                  # needs Docker: real-Postgres behaviour checks
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
