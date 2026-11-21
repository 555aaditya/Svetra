//! Leaked keys in the project folder (docs/doctor.md, Leaked keys in the project folder).
//!
//! A key is judged by what it decodes to or by its prefix, never by its variable name or by
//! looking random. The anon and publishable keys are public by design and never reported. A
//! finding never carries the value: it names a file, a line and a kind of key.

use super::files::{File, Git};
use super::jwt::{self, Role};
use super::{Finding, Severity};
use std::collections::HashSet;

/// What one line holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Hit {
    ServiceKey,
    UnrecognizedJwt,
}

/// Environment variable name prefixes whose values frameworks put into the browser bundle.
const PUBLIC_PREFIXES: [&str; 6] = [
    "NEXT_PUBLIC_",
    "VITE_",
    "REACT_APP_",
    "EXPO_PUBLIC_",
    "PUBLIC_",
    "NUXT_PUBLIC_",
];

/// Runs every key check over `files`. `git` is `None` when the folder isn't a repository: then
/// every secret is treated as if its file were tracked.
pub fn check(files: &[File], git: Option<&Git>) -> Vec<Finding> {
    let mut out = Vec::new();
    for file in files {
        let tracked = git.is_none_or(|g| g.tracked(&file.path));
        let front_file = is_front_end(&file.path);
        let mut seen: HashSet<(Hit, usize)> = HashSet::new();
        let mut unrecognized = false;
        for (i, line) in file.text.lines().enumerate() {
            let line_no = i + 1;
            let exposed = tracked || front_file || is_public_variable(line);
            for hit in scan_line(line) {
                if !exposed || !seen.insert((hit, line_no)) {
                    continue;
                }
                match hit {
                    Hit::ServiceKey => out.push(service_key(&file.path, line_no, front_file)),
                    Hit::UnrecognizedJwt => {
                        if !unrecognized && !is_test_path(&file.path) {
                            unrecognized = true;
                            out.push(jwt_unrecognized(&file.path, line_no));
                        }
                    }
                }
            }
        }
    }
    out
}

fn scan_line(line: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    for token in tokens(line) {
        if let Some(tail) = token.strip_prefix("sb_secret_") {
            if tail.len() >= 16 && !is_placeholder(tail) {
                hits.push(Hit::ServiceKey);
            }
        } else if token.starts_with("eyJ") {
            match jwt::role(token) {
                Some(Role::Service) => hits.push(Hit::ServiceKey),
                Some(Role::Other) => hits.push(Hit::UnrecognizedJwt),
                Some(Role::Anon) | None => {}
            }
        }
    }
    hits
}

/// The runs of characters a key or token is made of.
fn tokens(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
        .map(|t| t.trim_end_matches('.'))
        .filter(|t| !t.is_empty())
}

/// An empty value, a reference or a made-up example: never a real secret.
fn is_placeholder(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    let mut chars = v.chars();
    let repeated = chars.next().is_some_and(|first| chars.all(|c| c == first));
    v.is_empty()
        || repeated
        || v.starts_with(['<', '$', '['])
        || v.starts_with("env(")
        || ["your", "xxxx", "example", "placeholder", "changeme"]
            .iter()
            .any(|p| v.contains(p))
}

/// Whether a file ships to the browser (docs/doctor.md, Where doctor looks).
fn is_front_end(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let dirs = &parts[..parts.len() - 1];
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    if dirs
        .iter()
        .any(|d| matches!(*d, "public" | "static" | "www"))
        || ext == "html"
    {
        return true;
    }
    matches!(
        ext,
        "js" | "jsx" | "ts" | "tsx" | "vue" | "svelte" | "astro"
    ) && !dirs
        .iter()
        .any(|d| matches!(*d, "server" | "api" | "functions" | "scripts"))
}

/// An environment variable put into the browser bundle on purpose, by its name.
fn is_public_variable(line: &str) -> bool {
    let line = line.trim_start();
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    PUBLIC_PREFIXES.iter().any(|p| line.starts_with(p))
}

fn is_test_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    ["test", "fixture", "mock", "__snapshots__"]
        .iter()
        .any(|w| p.contains(w))
}

/// What to do about any key that was in a tracked or front-end file.
const ROTATE: &str = "remove it from the code and rotate it now (Supabase dashboard, Settings, API; or the vendor's dashboard), and only then delete it from git. A key that was ever committed or shipped must be treated as stolen";
const SERVER_FUNCTION: &str = ". Do the work that needs it in a server function (a Supabase Edge Function, or your own backend), and have the front end call that with the signed-in person's token. The front end only needs the anon key";

fn rotate_fix(front_end: bool) -> String {
    if front_end {
        format!("{ROTATE}{SERVER_FUNCTION}.")
    } else {
        format!("{ROTATE}.")
    }
}

fn service_key(path: &str, line: usize, front_end: bool) -> Finding {
    Finding {
        kind: "service_key_exposed",
        severity: Severity::Critical,
        area: "keys",
        message: "This line holds your Supabase service role key. It skips every row-level security rule, and anyone who can read this file can read, change and delete every row.".to_string(),
        fix: rotate_fix(front_end),
        file: path.to_string(),
        line: Some(line),
    }
}

fn jwt_unrecognized(path: &str, line: usize) -> Finding {
    Finding {
        kind: "jwt_unrecognized",
        severity: Severity::Info,
        area: "keys",
        message: "This line holds a signed token Svetra doesn't recognize. If it's someone's session, it can be used to act as them.".to_string(),
        fix: "if it's a real session token, sign that person out; if it's a key, rotate it. Remove it from the file either way.".to_string(),
        file: path.to_string(),
        line: Some(line),
    }
}
