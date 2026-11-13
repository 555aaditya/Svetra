# svetra

**You design the data. Svetra makes it safe. The AI builds the rest.**

Svetra compiles a small `svetra.yaml` into a locked-down Supabase backend: Postgres tables with row-level security on every table, roles, sign-in settings, TypeScript and Zod types, and access tests that prove on your database who can see what.

```sh
npx svetra validate svetra.yaml
npx svetra compile svetra.yaml
npx svetra explain svetra.yaml
```

This package runs the prebuilt `svetra` binary. The first run downloads it from the GitHub release that matches this package's version, checks its SHA-256 against the release's `SHA256SUMS`, and caches it in `~/.cache/svetra` (`SVETRA_CACHE_DIR` overrides that). It has no dependencies and no install scripts. Prebuilt for macOS (Apple silicon and Intel), Linux (x86_64 and arm64) and Windows (x86_64). Elsewhere, use `cargo install svetra`.

**Behind a company proxy** that inspects HTTPS, such as Zscaler, the first run can't download the binary and says `unable to get local issuer certificate`. Point Node at the proxy's root certificate first, in the same terminal you start your agent from, because the MCP server downloads the same way:

```sh
export NODE_EXTRA_CA_CERTS=/path/to/proxy-root.pem
```

Your IT team can give you the certificate. On macOS it's usually in the System keychain: `security find-certificate -c "<proxy name>" -p /Library/Keychains/System.keychain > ~/proxy-root.pem`.

Website: https://555aaditya.github.io/Svetra/ · Source and docs: https://github.com/555aaditya/Svetra · MIT licence
