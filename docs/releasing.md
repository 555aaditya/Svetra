# Releasing

Releases are cut from `main` by the maintainer.

1. **Update the version:**
   1. In `Cargo.toml`, set the version.
   2. In `npm/package.json`, set the same version. `tests/npm.rs` fails if they differ.
   3. In `CHANGELOG.md`, rename "unreleased" to the release date.
   4. Merge them into `develop` through a PR.
2. **Publish `main`** by fast-forwarding it to `develop`: `git push origin develop:main`. Only the maintainer can push to `main`.
3. **Tag the release:** `git tag -s v0.1.0 main && git push origin v0.1.0`.

The tag starts `.github/workflows/release.yml`, which runs four jobs:

| Job | What it does |
|---|---|
| `build` | Builds `svetra` for Linux (x86_64 and aarch64), macOS (Apple Silicon and Intel) and Windows (x86_64) |
| `release` | Creates the GitHub Release with every build, a `SHA256SUMS` file, and the notes from this version's `CHANGELOG.md` section |
| `smoke` | Installs from the new release with `install.sh` on Linux and macOS, then checks `svetra --version` and runs `svetra validate` |
| `crates` | Runs `cargo publish`, but **only if** the repository has a `CARGO_REGISTRY_TOKEN` secret. Otherwise it skips, so a release never fails for lack of a token |

**Before the first crates.io publish:**
- Sign in to crates.io with the maintainer's GitHub account.
- Create an API token scoped to publishing `svetra`.
- Add it as a repository secret named `CARGO_REGISTRY_TOKEN`.

**Dry run:** push a tag with a suffix, such as `v0.1.0-rc.1`. It runs everything except crates.io, and creates a *pre-release*, which never becomes the "latest" the installer uses. Delete the pre-release and tag afterwards.

**npm** (`npx svetra`): `npm/` is a small wrapper with no dependencies. On first run it downloads the binary from the GitHub release whose tag matches its own version, checks it against `SHA256SUMS`, and caches it. Publish it **after** the release workflow has finished, because before that its first run has nothing to download:

```sh
cd npm
npm publish --access public
```

npm asks for a one-time code from the maintainer's authenticator app. Publishing isn't automated yet.
