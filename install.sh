#!/bin/sh
# Install svetra from GitHub Releases, checking its SHA-256 checksum.
#
#   curl -fsSL https://raw.githubusercontent.com/555aaditya/Svetra/main/install.sh | sh
#
# SVETRA_VERSION      a tag such as v0.1.0 (default: the latest release)
# SVETRA_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
# SVETRA_BASE_URL     download from somewhere else, such as a mirror or a local test folder
set -eu

repo="555aaditya/Svetra"
version="${SVETRA_VERSION:-latest}"
dir="${SVETRA_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  *)
    echo "svetra: no prebuilt binary for $(uname -s) $(uname -m)." >&2
    echo "Install from source instead: cargo install --git https://github.com/$repo" >&2
    exit 1
    ;;
esac

if [ -n "${SVETRA_BASE_URL:-}" ]; then
  base="$SVETRA_BASE_URL"
elif [ "$version" = latest ]; then
  base="https://github.com/$repo/releases/latest/download"
else
  base="https://github.com/$repo/releases/download/$version"
fi

file="svetra-$target.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $file ($version)"
curl -fsSL "$base/$file" -o "$tmp/$file"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"

expected="$(grep " $file\$" "$tmp/SHA256SUMS" | cut -d ' ' -f 1)"
if [ -z "$expected" ]; then
  echo "svetra: $file is missing from SHA256SUMS; not installing." >&2
  exit 1
fi
if command -v sha256sum > /dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$file" | cut -d ' ' -f 1)"
else
  actual="$(shasum -a 256 "$tmp/$file" | cut -d ' ' -f 1)"
fi
if [ "$expected" != "$actual" ]; then
  echo "svetra: checksum mismatch for $file; not installing." >&2
  exit 1
fi

tar xzf "$tmp/$file" -C "$tmp"
mkdir -p "$dir"
cp "$tmp/svetra-$target/svetra" "$dir/svetra"
chmod 755 "$dir/svetra"
echo "Installed $("$dir/svetra" --version) to $dir/svetra"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "Add it to your PATH: export PATH=\"$dir:\$PATH\"" ;;
esac
