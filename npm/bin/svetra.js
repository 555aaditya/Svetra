#!/usr/bin/env node
// Runs the svetra binary for this computer. The first run downloads it from the GitHub release
// matching this package's version, checks its SHA-256 against the release's SHA256SUMS, and
// caches it, so later runs start instantly. No install scripts and no dependencies.
"use strict";

const { execFileSync, spawnSync } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { version } = require("../package.json");

const TARGETS = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
  "win32-x64": "x86_64-pc-windows-msvc",
};

function fail(message) {
  console.error(`svetra: ${message}`);
  process.exit(1);
}

async function download(url) {
  let res;
  try {
    res = await fetch(url);
  } catch (e) {
    fail(
      `couldn't download ${url}: ${e.cause?.message || e.message}. ` +
        "Behind a proxy that inspects HTTPS, point NODE_EXTRA_CA_CERTS at its certificate."
    );
  }
  if (!res.ok) fail(`couldn't download ${url} (HTTP ${res.status})`);
  return Buffer.from(await res.arrayBuffer());
}

async function install(target, exe) {
  const base =
    process.env.SVETRA_BASE_URL ||
    `https://github.com/555aaditya/Svetra/releases/download/v${version}`;
  const file = `svetra-${target}.${process.platform === "win32" ? "zip" : "tar.gz"}`;
  console.error(`svetra: downloading ${version} for ${target} (first run only)`);
  const [archive, sums] = await Promise.all([
    download(`${base}/${file}`),
    download(`${base}/SHA256SUMS`),
  ]);
  const line = sums
    .toString()
    .split("\n")
    .map((l) => l.trim().split(/\s+/))
    .find(([, name]) => name === file);
  const got = crypto.createHash("sha256").update(archive).digest("hex");
  if (!line || line[0] !== got) fail(`the checksum of ${file} doesn't match SHA256SUMS; not installing it`);

  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "svetra-"));
  try {
    fs.writeFileSync(path.join(tmp, file), archive);
    // tar reads .tar.gz everywhere, and .zip too on macOS and Windows 10 or later.
    execFileSync("tar", ["-xf", file], { cwd: tmp });
    fs.mkdirSync(path.dirname(exe), { recursive: true });
    // Copy, then rename, so a crash or a second run never sees half a binary.
    const partial = `${exe}.partial-${process.pid}`;
    fs.copyFileSync(path.join(tmp, `svetra-${target}`, path.basename(exe)), partial);
    fs.chmodSync(partial, 0o755);
    fs.renameSync(partial, exe);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

(async () => {
  const target = TARGETS[`${process.platform}-${process.arch}`];
  if (!target) {
    fail(`no prebuilt binary for ${process.platform} ${process.arch}. Install it with Rust instead: cargo install svetra`);
  }
  const cache = process.env.SVETRA_CACHE_DIR || path.join(os.homedir(), ".cache", "svetra");
  const exe = path.join(cache, version, process.platform === "win32" ? "svetra.exe" : "svetra");
  if (!fs.existsSync(exe)) await install(target, exe);
  const run = spawnSync(exe, process.argv.slice(2), { stdio: "inherit" });
  if (run.error) fail(run.error.message);
  process.exit(run.status ?? 1);
})();
