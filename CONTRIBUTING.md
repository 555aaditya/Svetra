# Contributing to Svetra

Thanks for your interest. Svetra is early, and the rules are simple.

## Branches

- `main` holds released code only. Only the maintainer publishes to it.
- `develop` is the default branch. Open every PR against `develop`.
- Name your branch `feat/<name>`, `fix/<name>` or `docs/<name>`, and cut it from `develop`.

## Commits and PRs

- One logical change per commit. Keep commits small.
- The commit subject is one imperative line, under 72 characters. For example: `Add validate command`.
- One topic per PR. PRs are usually squash-merged, so the PR title becomes the commit subject. Write it the same way. PRs whose commits are already small and clean may be rebase-merged instead.
- Only the maintainer (@555aaditya) reviews and merges PRs.

## Before you start

Open an issue first for anything larger than a small fix, so we can agree on the approach before you write code.

## Pull requests that fall behind

When `develop` (or any base branch) moves, the `update-prs` workflow merges it into every open pull request based on it and reruns `ci`. `CHANGELOG.md` merges as a union (`.gitattributes`), so changelog lines never conflict. A real conflict is never resolved by the workflow: the pull request gets one comment listing the files, and a person merges or rebases it.
