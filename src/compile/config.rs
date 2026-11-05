//! Sign-in settings in `supabase/config.toml` (docs/compiler.md, Sign-in settings). The file
//! belongs to the Supabase CLI and to the user, so only the keys Svetra owns are set, one line
//! at a time, and every other line and comment is kept as it was.

use crate::model::Document;

/// One owned key: `table` is the TOML table name, such as `auth.email`, and `value` is TOML.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub table: String,
    pub key: String,
    pub value: String,
}

/// The keys to set for `doc`, given the current file. Empty without an `auth` section.
pub fn settings(doc: &Document, existing: &str) -> Vec<Setting> {
    let Some(a) = &doc.auth else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut set = |table: &str, key: &str, value: String| {
        out.push(Setting {
            table: table.into(),
            key: key.into(),
            value,
        })
    };
    let mfa = doc.roles.iter().any(|r| r.mfa);
    set("auth", "enable_signup", (!a.invite_only()).to_string());
    set("auth", "enable_anonymous_sign_ins", "false".into());
    set(
        "auth",
        "minimum_password_length",
        a.min_length().to_string(),
    );
    set(
        "auth",
        "password_requirements",
        format!("\"{}\"", a.requirements()),
    );
    set("auth.email", "enabled", a.email().to_string());
    set(
        "auth.email",
        "enable_confirmations",
        a.confirms_email().to_string(),
    );
    set("auth.email", "secure_password_change", "true".into());
    set("auth.mfa.totp", "enroll_enabled", mfa.to_string());
    set("auth.mfa.totp", "verify_enabled", mfa.to_string());
    let providers: Vec<&str> = a
        .sign_in
        .iter()
        .map(|m| m.name.as_str())
        .filter(|m| *m != "email")
        .collect();
    for p in &providers {
        let table = format!("auth.external.{p}");
        let env = format!("SUPABASE_AUTH_EXTERNAL_{}", p.to_uppercase());
        set(&table, "enabled", "true".into());
        set(&table, "client_id", format!("\"env({env}_CLIENT_ID)\""));
        set(&table, "secret", format!("\"env({env}_SECRET)\""));
    }
    for t in tables(existing) {
        if let Some(p) = t.strip_prefix("auth.external.")
            && !providers.contains(&p)
        {
            set(&t, "enabled", "false".into());
        }
    }
    out
}

/// A table header line, `[name]`, outside comments. Arrays of tables (`[[name]]`) don't count.
fn header(line: &str) -> Option<&str> {
    let t = line.trim();
    (t.starts_with('[') && !t.starts_with("[[") && t.ends_with(']'))
        .then(|| t[1..t.len() - 1].trim())
}

/// A `key = value` line, returning the key and the raw value.
fn entry(line: &str) -> Option<(&str, &str)> {
    let t = line.trim();
    if t.starts_with('#') {
        return None;
    }
    let (k, v) = t.split_once('=')?;
    Some((k.trim(), v.trim()))
}

fn tables(src: &str) -> Vec<String> {
    src.lines().filter_map(header).map(String::from).collect()
}

/// The line range of `table`'s body: after its header, up to the next header.
fn body(lines: &[String], table: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|l| header(l) == Some(table))? + 1;
    let end = lines[start..]
        .iter()
        .position(|l| header(l).is_some())
        .map_or(lines.len(), |i| start + i);
    Some((start, end))
}

/// The line index and raw value of `key` in `table`.
fn find(lines: &[String], table: &str, key: &str) -> Option<(usize, String)> {
    let (start, end) = body(lines, table)?;
    (start..end)
        .find_map(|i| entry(&lines[i]).and_then(|(k, v)| (k == key).then(|| (i, v.to_string()))))
}

/// `existing` with every setting applied. A missing key goes at the end of its table, after
/// the table's last key; a missing table goes at the end of the file.
pub fn apply(existing: &str, settings: &[Setting]) -> String {
    let mut lines: Vec<String> = existing.lines().map(String::from).collect();
    for s in settings {
        let line = format!("{} = {}", s.key, s.value);
        if let Some((i, _)) = find(&lines, &s.table, &s.key) {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            lines[i] = format!("{indent}{line}");
        } else if let Some((start, end)) = body(&lines, &s.table) {
            let last = (start..end).rev().find(|&i| entry(&lines[i]).is_some());
            lines.insert(last.map_or(start, |i| i + 1), line);
        } else {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{}]", s.table));
            lines.push(line);
        }
    }
    lines.join("\n") + "\n"
}

/// One line per setting that `existing` doesn't already have, for `compile --check`.
pub fn differences(existing: &str, settings: &[Setting]) -> Vec<String> {
    let lines: Vec<String> = existing.lines().map(String::from).collect();
    settings
        .iter()
        .filter_map(|s| {
            let name = format!("{}.{}", s.table, s.key);
            match find(&lines, &s.table, &s.key) {
                Some((_, v)) if v == s.value => None,
                Some((_, v)) => Some(format!("{name} is {v}, svetra.yaml says {}", s.value)),
                None => Some(format!("{name} is missing, svetra.yaml says {}", s.value)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# Supabase\n[auth]\nenabled = true\nminimum_password_length = 6\n\n# Comment kept\n[auth.email]\nenable_confirmations = false\n\n[auth.external.apple]\nenabled = true\nclient_id = \"\"\n";

    fn doc(auth: &str) -> Document {
        let src = format!(
            "svetra: \"0.1\"\nsystem: App\n{auth}entities:\n  A:\n    private: true\n    fields:\n      id: {{ type: uuid, primary: true }}\n"
        );
        crate::load::load(&src).0
    }

    #[test]
    fn owned_keys_change_and_everything_else_stays() {
        let d = doc("auth:\n  sign_in: [email, google]\n");
        let s = settings(&d, FILE);
        let out = apply(FILE, &s);
        assert!(out.contains("# Supabase\n[auth]\nenabled = true\nminimum_password_length = 10\n"));
        assert!(out.contains("# Comment kept\n[auth.email]\nenable_confirmations = true\nenabled = true\nsecure_password_change = true\n"));
        // Apple was enabled in the file but isn't in sign_in.
        assert!(out.contains("[auth.external.apple]\nenabled = false\nclient_id = \"\"\n"));
        assert!(out.contains("[auth.external.google]\nenabled = true\nclient_id = \"env(SUPABASE_AUTH_EXTERNAL_GOOGLE_CLIENT_ID)\"\nsecret = \"env(SUPABASE_AUTH_EXTERNAL_GOOGLE_SECRET)\"\n"));
        assert!(differences(&out, &s).is_empty());
        assert_eq!(apply(&out, &s), out, "applying twice changes nothing");
    }

    #[test]
    fn differences_name_each_key() {
        let d = doc("auth:\n  sign_in: [email]\n");
        let s = settings(&d, FILE);
        let diff = differences(FILE, &s);
        assert!(
            diff.contains(&"auth.minimum_password_length is 6, svetra.yaml says 10".to_string())
        );
        assert!(diff.contains(
            &"auth.email.secure_password_change is missing, svetra.yaml says true".to_string()
        ));
    }

    #[test]
    fn no_auth_section_means_no_settings() {
        assert!(settings(&doc(""), FILE).is_empty());
    }
}
