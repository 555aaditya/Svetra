# svetra

**You design the data. Svetra makes it safe. The AI builds the rest.**

Svetra compiles a small `svetra.yaml` into a locked-down Supabase backend: Postgres tables with row-level security on every table, roles, sign-in settings, TypeScript and Zod types, and access tests that prove on your database who can see what.

```sh
npx svetra validate svetra.yaml
npx svetra compile svetra.yaml
npx svetra explain svetra.yaml
```

This package runs the prebuilt `svetra` binary. The first run downloads it from the GitHub release that matches this package's version, checks its SHA-256 against the release's `SHA256SUMS`, and caches it in `~/.cache/svetra` (`SVETRA_CACHE_DIR` overrides that). It has no dependencies and no install scripts. Prebuilt for macOS (Apple silicon and Intel), Linux (x86_64 and arm64) and Windows (x86_64). Elsewhere, use `cargo install svetra`.

Website: https://555aaditya.github.io/Svetra/ · Source and docs: https://github.com/555aaditya/Svetra · MIT licence
