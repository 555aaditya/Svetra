//! Leaked keys in the project folder (docs/doctor.md, Leaked keys in the project folder).
//!
//! A key is judged by what it decodes to or by its prefix, never by its variable name or by
//! looking random. The anon and publishable keys are public by design and never reported. A
//! finding never carries the value: it names a file, a line and a kind of key.

use super::files::{File, Git};
use super::jwt::{self, Role};
use super::{Finding, Severity};
use crate::project;
use std::collections::HashSet;

/// What one line holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Hit {
    ServiceKey,
    UnrecognizedJwt,
    /// An index into `VENDORS`.
    Secret(usize),
    JwtSecret,
    DbPassword,
}

/// How the part of a token after its prefix must look.
enum Tail {
    /// Letters and digits, at least this many.
    Alnum(usize),
    /// Letters, digits and underscores, at least this many.
    Word(usize),
    /// Exactly this many hex digits.
    Hex(usize),
    /// Exactly this many capital letters and digits.
    Upper(usize),
    /// Not a prefix: the line holds a PEM private key header.
    Pem,
}

/// Other secrets: formats specific enough to give almost no false alarms. UNVERIFIED: the exact
/// lengths and prefixes, to be checked against each vendor's documentation. Adding a vendor is a
/// row here and a test. The private key is last: `scan_line` finds it by its header.
struct Vendor {
    prefix: &'static str,
    label: &'static str,
    risk: &'static str,
    severity: Severity,
    tail: Tail,
}

const VENDORS: [Vendor; 9] = [
    Vendor {
        prefix: "sk_live_",
        label: "a Stripe live secret key",
        risk: "Anyone who can read this file can charge and refund your customers.",
        severity: Severity::Critical,
        tail: Tail::Alnum(20),
    },
    Vendor {
        prefix: "rk_live_",
        label: "a Stripe live restricted key",
        risk: "Anyone who can read this file can do whatever the key was allowed to with your customers' money.",
        severity: Severity::Critical,
        tail: Tail::Alnum(20),
    },
    Vendor {
        prefix: "whsec_",
        label: "a Stripe webhook signing secret",
        risk: "Anyone who can read this file can forge payment events your app would trust.",
        severity: Severity::Critical,
        tail: Tail::Alnum(20),
    },
    Vendor {
        prefix: "sk_test_",
        label: "a Stripe test secret key",
        risk: "It can't move real money, but it gives full access to your Stripe test data.",
        severity: Severity::Warning,
        tail: Tail::Alnum(20),
    },
    Vendor {
        prefix: "AKIA",
        label: "an AWS access key id",
        risk: "With its secret it gives access to your AWS account.",
        severity: Severity::Critical,
        tail: Tail::Upper(16),
    },
    Vendor {
        prefix: "ghp_",
        label: "a GitHub personal access token",
        risk: "Anyone who can read this file can act as you on GitHub.",
        severity: Severity::Critical,
        tail: Tail::Alnum(30),
    },
    Vendor {
        prefix: "github_pat_",
        label: "a GitHub personal access token",
        risk: "Anyone who can read this file can act as you on GitHub.",
        severity: Severity::Critical,
        tail: Tail::Word(22),
    },
    Vendor {
        prefix: "sbp_",
        label: "a Supabase personal access token",
        risk: "Anyone who can read this file can manage your Supabase projects, including their data.",
        severity: Severity::Critical,
        tail: Tail::Hex(40),
    },
    Vendor {
        prefix: "",
        label: "a private key",
        risk: "Anyone who can read this file can pass as whoever or whatever it identifies.",
        severity: Severity::Critical,
        tail: Tail::Pem,
    },
];

impl Tail {
    fn matches(&self, tail: &str) -> bool {
        let all = |f: fn(char) -> bool| tail.chars().all(f);
        match self {
            Tail::Alnum(n) => tail.len() >= *n && all(|c| c.is_ascii_alphanumeric()),
            Tail::Word(n) => tail.len() >= *n && all(|c| c.is_ascii_alphanumeric() || c == '_'),
            Tail::Hex(n) => tail.len() == *n && all(|c| c.is_ascii_hexdigit()),
            Tail::Upper(n) => {
                tail.len() == *n && all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            }
            Tail::Pem => false,
        }
    }
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
        let first = out.len();
        let mut seen: HashSet<(Hit, usize)> = HashSet::new();
        let mut unrecognized = false;
        for (i, line) in file.text.lines().enumerate() {
            let line_no = i + 1;
            let exposed = tracked || front_file || is_public_variable(line);
            for hit in scan_line(line, &file.path) {
                if !exposed || !seen.insert((hit, line_no)) {
                    continue;
                }
                match hit {
                    Hit::ServiceKey => out.push(service_key(&file.path, line_no, front_file)),
                    Hit::JwtSecret => out.push(jwt_secret(&file.path, line_no, front_file)),
                    Hit::DbPassword => out.push(db_password(&file.path, line_no, front_file)),
                    Hit::Secret(v) => {
                        out.push(secret(&file.path, line_no, &VENDORS[v], front_file));
                    }
                    Hit::UnrecognizedJwt => {
                        if !unrecognized && !is_test_path(&file.path) {
                            unrecognized = true;
                            out.push(jwt_unrecognized(&file.path, line_no));
                        }
                    }
                }
            }
        }
        if file.path == project::CONFIG {
            for (i, line) in file.text.lines().enumerate() {
                if let Some(key) = config_secret_key(line) {
                    out.push(config_secret(&file.path, i + 1, &key, tracked));
                }
            }
        }
        if let Some(git) = git.filter(|_| is_env_file(&file.path)) {
            if git.tracked(&file.path) {
                let critical = out[first..]
                    .iter()
                    .any(|f| f.severity == Severity::Critical);
                let values = non_public_values(&file.text);
                if critical || values > 0 {
                    out.push(env_committed(&file.path, values, critical));
                }
            } else if !git.ignored(&file.path) {
                out.push(env_not_ignored(&file.path));
            }
        }
    }
    out
}

/// The key on a `supabase/config.toml` line that is named like a secret and holds a literal
/// string instead of `env(NAME)`. `jwt_secret` is `jwt_secret_exposed`'s.
fn config_secret_key(line: &str) -> Option<String> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim().to_ascii_lowercase();
    if key.starts_with('#') || key.starts_with('[') || key == "jwt_secret" {
        return None;
    }
    let named_like_a_secret = key.contains("secret")
        || key == "pass"
        || key == "password"
        || key.ends_with("_password")
        || key.contains("api_key")
        || key.contains("auth_token");
    let value = value.trim().strip_prefix('"')?.split('"').next()?;
    (named_like_a_secret && !is_placeholder(value)).then_some(key)
}

fn config_secret(path: &str, line: usize, key: &str, tracked: bool) -> Finding {
    Finding {
        kind: "config_secret",
        severity: if tracked {
            Severity::Critical
        } else {
            Severity::Warning
        },
        area: "keys",
        message: format!(
            "The key `{key}` holds a secret in plain text{}.",
            if tracked {
                ", in a file that is committed (or that doctor can't tell isn't)"
            } else {
                ""
            }
        ),
        fix: format!(
            "use `env(NAME)` instead, for example `{key} = \"env(SUPABASE_AUTH_SECRET)\"`, and keep the value in a .env file that isn't committed. If it was ever committed, rotate it."
        ),
        file: path.to_string(),
        line: Some(line),
    }
}

/// A dotenv file: `.env`, `.env.local`, `.env.production` and so on, but not the examples that
/// are committed on purpose.
pub fn is_env_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    (name == ".env" || name.starts_with(".env."))
        && !name.ends_with(".example")
        && !name.ends_with(".sample")
        && !name.ends_with(".template")
}

/// How many values in a dotenv file aren't public by design: a name with `SECRET`, `PASSWORD`,
/// `TOKEN`, `PRIVATE` or `KEY` whose value isn't the anon or publishable key, or a value that is
/// a service key whatever its name. Empty values and placeholders don't count.
fn non_public_values(text: &str) -> usize {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line);
            if line.starts_with('#') {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            let value = value.trim().trim_matches(['"', '\'']);
            if is_placeholder(value) {
                return None;
            }
            let name = name.trim().to_ascii_uppercase();
            let public =
                value.starts_with("sb_publishable_") || jwt::role(value) == Some(Role::Anon);
            let secret_value =
                value.starts_with("sb_secret_") || jwt::role(value) == Some(Role::Service);
            let sensitive = ["SECRET", "PASSWORD", "TOKEN", "PRIVATE", "KEY"]
                .iter()
                .any(|w| name.contains(w));
            ((sensitive && !public) || secret_value).then_some(())
        })
        .count()
}

fn env_committed(path: &str, values: usize, critical: bool) -> Finding {
    let held = match values {
        0 => String::new(),
        1 => " It holds 1 value that isn't public by design.".to_string(),
        n => format!(" It holds {n} values that aren't public by design."),
    };
    Finding {
        kind: "env_committed",
        severity: if critical {
            Severity::Critical
        } else {
            Severity::Warning
        },
        area: "keys",
        message: format!(
            "This file is committed to git.{held} Deleting it now doesn't remove it from history."
        ),
        fix: format!(
            "run `git rm --cached {path}`, add it to .gitignore, and rotate every secret it held: it is in git's history, and on GitHub once pushed."
        ),
        file: path.to_string(),
        line: None,
    }
}

fn env_not_ignored(path: &str) -> Finding {
    Finding {
        kind: "env_not_ignored",
        severity: Severity::Warning,
        area: "keys",
        message: "This file isn't in .gitignore. One careless `git add .` would commit it."
            .to_string(),
        fix: "add `.env*` to .gitignore, then `!.env.example` if you keep an example file."
            .to_string(),
        file: path.to_string(),
        line: None,
    }
}

/// The value assigned to `name` on this line (`NAME=value`, `NAME: "value"`, `"NAME": "value"`),
/// without its quotes. `colon` also accepts `:`, for YAML and JSON files.
fn assigned<'a>(line: &'a str, name: &str, colon: bool) -> Option<&'a str> {
    let at = line.find(name)?;
    if line[..at]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let rest = line[at + name.len()..].trim_start_matches(['"', '\'']);
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix('=')
        .or_else(|| rest.strip_prefix(':').filter(|_| colon))?
        .trim_start();
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''));
    let value = match quote {
        Some(q) => rest[1..].split(q).next().unwrap_or(""),
        None => rest.split([' ', '\t', '#', ',', ';']).next().unwrap_or(""),
    };
    Some(value)
}

/// Supabase's local development JWT secret, public in its documentation.
const LOCAL_JWT_SECRET: &str = "super-secret-jwt-token-with-at-least-32-characters-long";

/// A connection string with a password, as `postgres://user:password@host/db`: whether it has a
/// real one. The local database and placeholders don't count.
fn has_db_password(line: &str) -> bool {
    ["postgres://", "postgresql://"].iter().any(|scheme| {
        line.match_indices(scheme).any(|(at, _)| {
            let url = &line[at + scheme.len()..];
            let url = url
                .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>'))
                .next()
                .unwrap_or("");
            let Some((user_info, host)) = url.rsplit_once('@') else {
                return false;
            };
            let Some((_, password)) = user_info.split_once(':') else {
                return false;
            };
            let host = host.split(['/', ':', '?']).next().unwrap_or("");
            !is_placeholder(password)
                && !matches!(password, "password" | "postgres")
                && !matches!(host, "localhost" | "127.0.0.1" | "[::1]")
        })
    })
}

fn scan_line(line: &str, path: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    let colon = path.ends_with(".yml") || path.ends_with(".yaml") || path.ends_with(".json");
    let jwt_secret = assigned(line, "SUPABASE_JWT_SECRET", colon).or_else(|| {
        if path.ends_with("config.toml") {
            assigned(line, "jwt_secret", false)
        } else {
            None
        }
    });
    if jwt_secret.is_some_and(|v| !is_placeholder(v) && v != LOCAL_JWT_SECRET) {
        hits.push(Hit::JwtSecret);
    }
    if has_db_password(line) {
        hits.push(Hit::DbPassword);
    }
    // The two halves of a PEM header, apart so that this file doesn't match itself.
    const BEGIN: &str = "-----BEGIN ";
    const END: &str = "PRIVATE KEY-----";
    // Between them, only the key's kind in capitals (`RSA `, `EC `, ``): prose that writes
    // `BEGIN ... PRIVATE KEY` isn't a key.
    let pem = line.find(BEGIN).is_some_and(|at| {
        let rest = &line[at + BEGIN.len()..];
        rest.find(END).is_some_and(|end| {
            rest[..end]
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == ' ')
        })
    });
    if pem {
        hits.push(Hit::Secret(VENDORS.len() - 1));
    }
    for token in tokens(line) {
        if let Some(tail) = token.strip_prefix("sb_secret_") {
            if tail.len() >= 16 && !is_placeholder(tail) {
                hits.push(Hit::ServiceKey);
            }
        } else if let Some(v) = VENDORS.iter().position(|v| {
            !v.prefix.is_empty()
                && token
                    .strip_prefix(v.prefix)
                    .is_some_and(|t| v.tail.matches(t) && !is_placeholder(t))
        }) {
            hits.push(Hit::Secret(v));
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

fn jwt_secret(path: &str, line: usize, front_end: bool) -> Finding {
    Finding {
        kind: "jwt_secret_exposed",
        severity: Severity::Critical,
        area: "keys",
        message: "This line holds your project's JWT secret. With it anyone can sign in as any user, admins included.".to_string(),
        fix: format!(
            "{} In config.toml, write `env(SUPABASE_JWT_SECRET)` and keep the value in a .env file that isn't committed.",
            rotate_fix(front_end)
        ),
        file: path.to_string(),
        line: Some(line),
    }
}

fn db_password(path: &str, line: usize, front_end: bool) -> Finding {
    Finding {
        kind: "db_password_exposed",
        severity: Severity::Critical,
        area: "keys",
        message: "This line holds your database password. It gives full access to every table."
            .to_string(),
        fix: format!(
            "{} Change the database password (Supabase dashboard, Settings, Database) and keep the connection string in an environment variable.",
            rotate_fix(front_end)
        ),
        file: path.to_string(),
        line: Some(line),
    }
}

fn secret(path: &str, line: usize, vendor: &Vendor, front_end: bool) -> Finding {
    Finding {
        kind: "secret_exposed",
        severity: vendor.severity,
        area: "keys",
        message: format!("This line holds {}. {}", vendor.label, vendor.risk),
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
