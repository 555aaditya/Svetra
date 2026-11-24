# The GitHub Action for `svetra plan`, version 0.1 (draft)

This document specifies a GitHub Action that runs [`svetra plan`](change.md) on every pull request that changes `svetra.yaml`, and posts the result on the PR: what happens to the data, and **who gains or loses access**. The access lines are the part a reviewer needs and a schema diff can't show.

**Done when:** a PR that widens a policy and removes an entity gets one comment that names both in plain words, the check fails until a person has agreed to them, and a later push updates the same comment instead of adding another.

## Why

A change to `svetra.yaml` is one line in a diff, and "everyone can now see every booking" is also one line. Reviewers skim it. `plan` already says it in words, but only to whoever runs it. The Action runs it for everyone, on the PR, before merge.

## What it adds to the CLI

The Action needs three small things from `plan` and `diff`. They are useful on their own.

| Flag | Meaning |
|---|---|
| `--from-git REF` | The design the database has is `svetra.yaml` as it is at git `REF` (`git show REF:<path>`), such as `origin/develop`. In CI it is the PR's base. Mutually exclusive with `--from`. The plan's "from" name is `REF:<path>`. A `REF` that doesn't exist, or a file not at it, is a usage error (exit `2`) that names both |
| `--no-write` | `plan` only. Never writes the rename migration; the plan lists the file it would have written. CI must not write into the checkout |
| `--markdown` | The plan as a comment: see Comment. Not with `--json` |

`--from-git` closes open decision C1 in change.md.

## Using it

```yaml
# .github/workflows/svetra.yml
name: svetra
on:
  pull_request:
    paths: ["svetra.yaml"]
permissions:
  contents: read
  pull-requests: write
jobs:
  plan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with: { fetch-depth: 0 }
      - uses: 555aaditya/Svetra@v0.9.0
```

| Input | Default | Meaning |
|---|---|---|
| `file` | `svetra.yaml` | The design, relative to the repository root |
| `base` | the PR's base branch | The git ref the database's design is read from. Needs history: `fetch-depth: 0`, or fetch that ref |
| `accept` | none | Kinds a person has agreed to, as in `plan --accept`: `wider_access,remove_entity` |
| `version` | `latest` | The Svetra release to install, such as `v0.9.0`. It must have `--from-git`, so 0.9.0 or later. Pin it, so a PR can't change what runs between two pushes |
| `comment` | `true` | Post the comment. When `false`, or when the token can't write (a PR from a fork), the plan goes to the job summary instead |
| `token` | `github.token` | Used only to post the comment |

| Output | Meaning |
|---|---|
| `needs_yes` | The kinds still waiting for a yes, comma separated; empty when none |
| `changes` | How many changes the plan lists |

## What it does

1. **Installs Svetra** with `install.sh` at `version`, which checks the release's SHA-256 before use.
2. **Skips quietly** when `file` is not at `base` (a new design has nothing to compare with) or is unchanged: it says so in the summary, posts nothing, and passes.
3. **Runs** `svetra plan <file> --from-git <base> --no-write --markdown`, with `--accept` for the `accept` input. Exit `1` from `plan` (something needs a yes) fails the job; exit `2` (a usage error or an invalid design) fails it too, with the message in the log.
4. **Posts one comment per PR.** The comment starts with a hidden marker, `<!-- svetra-plan -->`. If the PR already has a comment with the marker, it is edited, not added to; if the token isn't allowed to edit it, a new one is added. Nothing else on the PR is touched.
5. **Writes the same text to the job summary,** so a PR from a fork, where the token can't comment, still shows it.

## Comment

```
### svetra plan: 3 changes to Bookings (from origin/develop:svetra.yaml)

| | Change |
|---|---|
| safe | add field Booking.notes |
| destructive | remove entity Comment: every comment is deleted |
| wider | A signed-in person can now see some locations (was: none). |

**Needs a yes:** `remove_entity`, `wider_access`. A person who owns the data has to agree, then add them to `accept` in the workflow.
```

- One row per change, in `plan`'s order, with the tag `plan` prints. The `because` and `hint` lines follow the row, indented, as `<br>` lines in the same cell.
- With nothing waiting: "**Nothing needs a yes.**" With every waiting kind accepted: "**Accepted:** …".
- Text comes from names that the loader has already limited to letters, digits and underscores, and from fixed sentences. Nothing from the PR's text is passed through as markdown, and `|` and backticks in a message are escaped.

## Trust

- **Use `pull_request`, never `pull_request_target`.** The Action runs the PR's own design, so it must run with the PR's own permissions. On a fork the token is read-only, the comment is skipped, and the summary carries the plan.
- **`accept` lives in the workflow file,** which is part of the PR's diff. A PR that adds `accept: wider_access` to its own workflow is visible in review, but the Action can't stop it. Protect the workflow file with CODEOWNERS, and make the check required, so that a person other than the author has to read the access lines before merge.
- **The Action reads and compares designs.** It applies nothing to a database and writes nothing to the checkout.

## Not in version 0.1

- `svetra check` or `doctor` in the same Action (`check` needs a database; scheduled monitoring is a separate Action).
- Acceptance by PR label or by a review from a named person.
- Checks on a design in a monorepo subfolder with more than one `svetra.yaml`: run the Action once per file.
- Publishing to the Marketplace.

## Tests

- **CLI, on a throwaway git repository:** `--from-git` against a base branch gives the same plan as `--from` with that file; a missing ref and a file missing at the ref each exit `2` and say which; `--no-write` writes nothing and lists the migration; `--markdown` matches a golden file for a plan with a rename, a destructive change and a wider-access change; `--from` with `--from-git` is a usage error.
- **The Action itself:** `tests/action.rs` checks that `action.yml` declares the documented inputs and outputs, and that no `${{ }}` expression sits inside a `run:` script (inputs reach scripts through `env`, so a branch name or input can't inject shell). The script was also run by hand against a throwaway repository with and without `accept`. Its first end-to-end run on GitHub needs a release that has the new flags.
