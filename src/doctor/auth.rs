//! Auth settings (docs/doctor.md, Auth settings): how people sign in, read from
//! `supabase/config.toml` and `svetra.yaml`. Read-only. The TOML reader is small on purpose: it
//! knows tables, `key = value` lines, comments, quoted strings, booleans, integers and string
//! arrays, and gives up (returning the line) on anything else, so the run never fails on it.

use super::{Finding, Severity};
use crate::compile::config::{entry, header};
use crate::model::Document;
use crate::project;
use std::path::Path;

const DESIGN: &str = "svetra.yaml";
const DASHBOARD: &str = "and check the same setting in the dashboard, under Authentication";

/// What the auth checks found, and what they could not do.
#[derive(Debug, Default)]
pub struct Auth {
    pub findings: Vec<Finding>,
    /// Why the config checks were skipped, for "Not checked".
    pub not_checked: Option<String>,
    /// There is no usable `svetra.yaml`, so who the admins are is a reminder.
    pub admin_reminder: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Val {
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<String>),
    Other,
}

/// A parsed `config.toml`: `(table, key, value, line)`.
struct Config(Vec<(String, String, Val, usize)>);

impl Config {
    fn get(&self, table: &str, key: &str) -> Option<(&Val, usize)> {
        self.0
            .iter()
            .find(|(t, k, _, _)| t == table && k == key)
            .map(|(_, _, v, l)| (v, *l))
    }

    fn bool(&self, table: &str, key: &str) -> Result<Option<(bool, usize)>, usize> {
        match self.get(table, key) {
            None => Ok(None),
            Some((Val::Bool(b), l)) => Ok(Some((*b, l))),
            Some((_, l)) => Err(l),
        }
    }
}

/// `line` without a trailing comment (a `#` outside quotes).
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '#') => return &line[..i],
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            _ => {}
        }
    }
    line
}

/// Net `[` minus `]` outside quotes.
fn depth(text: &str) -> i32 {
    let mut quote: Option<char> = None;
    let mut d = 0;
    for c in text.chars() {
        match (quote, c) {
            (None, '[') => d += 1,
            (None, ']') => d -= 1,
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            _ => {}
        }
    }
    d
}

fn unquote(raw: &str) -> Option<String> {
    let t = raw.trim();
    for q in ['"', '\''] {
        if t.len() >= 2 && t.starts_with(q) && t.ends_with(q) && !t.starts_with("\"\"\"") {
            let inner = &t[1..t.len() - 1];
            if !inner.contains(q) {
                return Some(inner.to_string());
            }
        }
    }
    None
}

fn value(raw: &str) -> Val {
    let t = raw.trim();
    if let Some(s) = unquote(t) {
        return Val::Str(s);
    }
    match t {
        "true" => return Val::Bool(true),
        "false" => return Val::Bool(false),
        _ => {}
    }
    if let Ok(n) = t.replace('_', "").parse::<i64>() {
        return Val::Int(n);
    }
    if t.starts_with('[') && t.ends_with(']') {
        let inner = &t[1..t.len() - 1];
        let mut items = Vec::new();
        for part in inner.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match unquote(part) {
                Some(s) => items.push(s),
                None => return Val::Other,
            }
        }
        return Val::List(items);
    }
    Val::Other
}

/// Reads `src`, or returns the 1-based line it did not understand.
fn parse(src: &str) -> Result<Config, usize> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    let mut table = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = strip_comment(lines[i]).trim();
        let at = i + 1;
        i += 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[[") && line.ends_with("]]") {
            table = format!("[[{}]]", &line[2..line.len() - 2]);
            continue;
        }
        if line.starts_with('[') {
            table = header(line).ok_or(at)?.to_string();
            continue;
        }
        let (key, raw) = entry(line).ok_or(at)?;
        let mut raw = raw.to_string();
        if raw.starts_with("\"\"\"") && !(raw.len() >= 6 && raw.ends_with("\"\"\"")) {
            // A multi-line string: skip to its end.
            loop {
                let Some(next) = lines.get(i) else {
                    return Err(at);
                };
                i += 1;
                if next.contains("\"\"\"") {
                    break;
                }
            }
            out.push((table.clone(), key.to_string(), Val::Other, at));
            continue;
        }
        // A multi-line array: keep reading until the brackets balance.
        while raw.starts_with('[') && depth(&raw) > 0 {
            let Some(next) = lines.get(i) else {
                return Err(at);
            };
            i += 1;
            raw.push(' ');
            raw.push_str(strip_comment(next).trim());
        }
        if key.is_empty() {
            return Err(at);
        }
        out.push((table.clone(), key.to_string(), value(&raw), at));
    }
    Ok(Config(out))
}

fn find(file: &str, line: Option<usize>, kind: &'static str, severity: Severity) -> Finding {
    Finding {
        kind,
        severity,
        area: "auth",
        message: String::new(),
        fix: String::new(),
        file: file.to_string(),
        line,
    }
}

/// The design, when `DIR/svetra.yaml` exists and has no errors.
fn design(dir: &Path) -> Option<Document> {
    let src = project::read(&dir.join(DESIGN)).ok()?;
    if crate::report::has_errors(&crate::report::validate(&src)) {
        return None;
    }
    Some(crate::load::load(&src).0)
}

/// The settings that run, from the config or, with no config, from the design.
struct Settings {
    /// `(value, file, line)`; `None` line for a default.
    email: bool,
    confirmations: bool,
    min_length: i64,
    anonymous: bool,
    signup: bool,
    file: &'static str,
    lines: [Option<usize>; 4],
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let h = rest.split(['/', '?', '#']).next().unwrap_or("");
    let h = h.rsplit('@').next().unwrap_or("");
    if h.starts_with('[') {
        return h.split(']').next().unwrap_or(h).trim_start_matches('[');
    }
    h.split(':').next().unwrap_or("")
}

fn is_local(url: &str) -> bool {
    matches!(host(url), "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
        || host(url).ends_with(".localhost")
}

/// Runs the auth checks on `dir`.
pub fn check(dir: &Path) -> Auth {
    let mut auth = Auth::default();
    let doc = design(dir);
    if let Some(d) = &doc {
        admin_no_mfa(d, &mut auth.findings);
    } else {
        auth.admin_reminder = true;
    }
    let config_path = dir.join(project::CONFIG);
    let config = match std::fs::read_to_string(&config_path) {
        Ok(src) => match parse(&src) {
            Ok(c) => Some(c),
            Err(line) => {
                auth.not_checked = Some(format!(
                    "{} was not understood (line {line}), so its sign-in settings were not read. Fix the line or run `svetra compile`.",
                    project::CONFIG
                ));
                return auth;
            }
        },
        Err(_) => None,
    };
    let a = doc.as_ref().and_then(|d| d.auth.as_ref());
    let settings = match (&config, a) {
        (Some(c), _) => match from_config(c) {
            Ok(s) => s,
            Err(line) => {
                auth.not_checked = Some(format!(
                    "{} has a sign-in setting doctor did not understand (line {line}), so those settings were not read.",
                    project::CONFIG
                ));
                return auth;
            }
        },
        (None, Some(a)) => Settings {
            email: a.email(),
            confirmations: a.confirms_email(),
            min_length: a.min_length(),
            anonymous: false,
            signup: !a.invite_only(),
            file: DESIGN,
            lines: [Some(a.pos.line); 4],
        },
        (None, None) => {
            auth.not_checked = Some(format!(
                "there is no {} and no `auth:` section in {DESIGN}. Run `supabase init` to make one.",
                project::CONFIG
            ));
            return auth;
        }
    };
    let from_config = config.is_some();
    let tail = if from_config {
        format!(", {DASHBOARD}")
    } else {
        String::new()
    };
    let (file, [l_email, l_len, l_anon, l_signup]) = (settings.file, settings.lines);
    if settings.email && !settings.confirmations {
        let mut f = find(file, l_email, "email_unconfirmed", Severity::Warning);
        f.message = "People can sign up with an email address they don't own, so anyone can claim anyone's address.".into();
        f.fix = if from_config {
            format!("set `enable_confirmations = true` under [auth.email] (or `confirm_email: true` in {DESIGN}){tail}.")
        } else {
            "set `confirm_email: true` and compile.".into()
        };
        auth.findings.push(f);
    }
    if settings.min_length < 8 {
        let mut f = find(file, l_len, "weak_password", Severity::Warning);
        f.message = format!(
            "Passwords can be as short as {} characters.",
            settings.min_length
        );
        f.fix = format!(
            "set `minimum_password_length = 10` and `password_requirements = \"lower_upper_letters_digits\"`{tail}."
        );
        auth.findings.push(f);
    }
    if settings.anonymous {
        let mut f = find(file, l_anon, "anonymous_sign_ins", Severity::Warning);
        f.message = "Anyone can get a signed-in account without giving an email. Every rule that allows signed-in people now allows them.".into();
        f.fix = format!("set `enable_anonymous_sign_ins = false` unless your app needs guests{tail}.");
        auth.findings.push(f);
    }
    if settings.signup {
        let mut f = find(file, l_signup, "open_signup", Severity::Info);
        f.message = "Anyone on the internet can create an account. Fine for a public app; for a private one, set `enable_signup = false` and invite people.".into();
        f.fix = format!("if the app is private, set `enable_signup = false` (or `signup: invite_only` in {DESIGN}){tail}.");
        auth.findings.push(f);
    }
    if let Some(c) = &config {
        if let Some(d) = &doc {
            mfa_off(d, c, &mut auth.findings);
        }
        redirect_open(c, &mut auth.findings);
    }
    auth
}

fn from_config(c: &Config) -> Result<Settings, usize> {
    let b = |t: &str, k: &str, default: bool| -> Result<(bool, Option<usize>), usize> {
        Ok(c.bool(t, k)?
            .map_or((default, None), |(v, l)| (v, Some(l))))
    };
    let (email, l_email) = b("auth.email", "enabled", true)?;
    let (confirmations, l_conf) = b("auth.email", "enable_confirmations", false)?;
    let (anonymous, l_anon) = b("auth", "enable_anonymous_sign_ins", false)?;
    let (signup, l_signup) = b("auth", "enable_signup", true)?;
    let (min_length, l_len) = match c.get("auth", "minimum_password_length") {
        None => (6, None),
        Some((Val::Int(n), l)) => (*n, Some(l)),
        Some((_, l)) => return Err(l),
    };
    // Where a finding points: the key that causes it, or the file when it's a default.
    Ok(Settings {
        email,
        confirmations,
        min_length,
        anonymous,
        signup,
        file: project::CONFIG,
        lines: [l_conf.or(l_email), l_len, l_anon, l_signup],
    })
}

fn admin_no_mfa(doc: &Document, out: &mut Vec<Finding>) {
    for r in &doc.roles {
        let n = r.name.to_lowercase();
        if r.mfa || !["admin", "owner", "superadmin", "root"].contains(&n.as_str()) {
            continue;
        }
        let mut f = find(DESIGN, Some(r.pos.line), "admin_no_mfa", Severity::Warning);
        f.message = format!(
            "The {} role works with a password alone. If a password leaks, so does everything {} can reach.",
            r.name, r.name
        );
        f.fix = "add `mfa: true` to the role and run `svetra compile`.".into();
        out.push(f);
    }
}

fn mfa_off(doc: &Document, c: &Config, out: &mut Vec<Finding>) {
    let off = ["enroll_enabled", "verify_enabled"]
        .iter()
        .find_map(|k| match c.get("auth.mfa.totp", k) {
            Some((Val::Bool(false), l)) => Some(l),
            _ => None,
        });
    let Some(line) = off else { return };
    for r in doc.roles.iter().filter(|r| r.mfa) {
        let mut f = find(project::CONFIG, Some(line), "mfa_off", Severity::Warning);
        f.message = format!(
            "The role {} needs a second factor, but authenticator apps are switched off, so nobody can reach it.",
            r.name
        );
        f.fix = format!("run `svetra compile`, {DASHBOARD}.");
        out.push(f);
    }
}

/// `redirect_open`. UNVERIFIED: the key names `site_url` and `additional_redirect_urls` are from
/// memory of the Supabase CLI's `config.toml` (docs/doctor.md), so the fix says so.
fn redirect_open(c: &Config, out: &mut Vec<Finding>) {
    let site = match c.get("auth", "site_url") {
        Some((Val::Str(s), l)) => Some((s.clone(), l)),
        _ => None,
    };
    let mut urls: Vec<(String, usize)> = site.iter().cloned().collect();
    if let Some((Val::List(l), line)) = c.get("auth", "additional_redirect_urls") {
        urls.extend(l.iter().map(|u| (u.clone(), line)));
    }
    let real_site = site.as_ref().is_some_and(|(s, _)| !is_local(s));
    for (url, line) in urls {
        let h = host(&url);
        let why = if h.contains('*') {
            "its host is a wildcard"
        } else if url.starts_with("http://") && !is_local(&url) {
            "it is plain http for a non-local address"
        } else if real_site && is_local(&url) {
            "it is a local address while site_url is a real domain"
        } else {
            continue;
        };
        let mut f = find(project::CONFIG, Some(line), "redirect_open", Severity::Warning);
        f.message = format!(
            "Sign-in links may send people to addresses you don't control: {url} ({why})."
        );
        f.fix = format!(
            "list only your own production addresses, {DASHBOARD}. UNVERIFIED: doctor reads the key names site_url and additional_redirect_urls from memory of the Supabase CLI's config.toml, not from a file `supabase init` made."
        );
        out.push(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tables_comments_quotes_and_arrays() {
        let c = parse("# top\n[auth] # c\nsite_url = \"http://x#y\" # tail\nminimum_password_length = 6\nenable_signup = true\nadditional_redirect_urls = [\n  \"a\", # one\n  'b',\n]\n[auth.email]\nenable_confirmations = false\n[[x]]\nk = 1\n").ok().unwrap();
        assert_eq!(
            c.get("auth", "site_url").map(|(v, _)| v.clone()),
            Some(Val::Str("http://x#y".into()))
        );
        assert_eq!(
            c.get("auth", "additional_redirect_urls").map(|(v, _)| v.clone()),
            Some(Val::List(vec!["a".into(), "b".into()]))
        );
        assert_eq!(c.bool("auth.email", "enable_confirmations"), Ok(Some((false, 11))));
    }

    #[test]
    fn gives_up_with_a_line_on_a_shape_it_does_not_know() {
        assert_eq!(parse("[auth]\nthis is not toml\n").err(), Some(2));
        assert_eq!(parse("[auth\n").err(), Some(1));
    }
}
