//! The project folder as doctor sees it: its text files, and what git says about them
//! (docs/doctor.md, Where doctor looks). Read-only: `git ls-files` and `git check-ignore` only.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const SKIP_DIRS: [&str; 7] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    "vendor",
];
const MAX_BYTES: u64 = 1_000_000;

/// A text file under the project folder.
pub struct File {
    /// Relative to the project folder, with `/` separators.
    pub path: String,
    pub text: String,
}

/// Every text file under `dir` up to 1 MB, in path order. Skips `.git`, `node_modules`, `target`,
/// `dist`, `build`, `.next`, `vendor`, lockfiles, binary files and every symbolic link, so
/// nothing outside `dir` is ever read.
pub fn walk(dir: &Path) -> Vec<File> {
    let mut files = Vec::new();
    visit(dir, "", &mut files);
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

fn visit(dir: &Path, prefix: &str, out: &mut Vec<File>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let path = format!("{prefix}{name}");
        if meta.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                visit(&entry.path(), &format!("{path}/"), out);
            }
        } else if meta.is_file() && meta.len() <= MAX_BYTES && !is_lockfile(&name) {
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            if bytes.contains(&0) {
                continue;
            }
            out.push(File {
                path,
                text: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
    }
}

fn is_lockfile(name: &str) -> bool {
    name.ends_with(".lock")
        || name.ends_with(".lockb")
        || matches!(name, "package-lock.json" | "pnpm-lock.yaml" | "go.sum")
}

/// What git says about the project's files.
pub struct Git {
    tracked: HashSet<String>,
    ignored: HashSet<String>,
}

impl Git {
    /// `None` when `dir` isn't in a git repository, or git can't be run.
    pub fn open(dir: &Path) -> Option<Git> {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["ls-files", "-z"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(Git {
            tracked: split_nul(&out.stdout),
            ignored: HashSet::new(),
        })
    }

    pub fn tracked(&self, path: &str) -> bool {
        self.tracked.contains(path)
    }

    pub fn ignored(&self, path: &str) -> bool {
        self.ignored.contains(path)
    }

    /// Asks `git check-ignore` which of the untracked `paths` are ignored.
    pub fn check_ignored(&mut self, dir: &Path, paths: &[&str]) {
        let paths: Vec<&str> = paths.iter().copied().filter(|p| !self.tracked(p)).collect();
        if paths.is_empty() {
            return;
        }
        let Ok(mut child) = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["check-ignore", "-z", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let input = paths.join("\0");
            // A write error means git quit early; its output so far is still what it said.
            let _ = stdin.write_all(input.as_bytes());
        }
        if let Ok(out) = child.wait_with_output() {
            self.ignored = split_nul(&out.stdout);
        }
    }
}

fn split_nul(bytes: &[u8]) -> HashSet<String> {
    bytes
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}
