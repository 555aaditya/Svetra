//! Render diagnostics for people (default) or agents (`--json`). Columns are 1-based here.

use crate::diag::{Code, Diagnostic, Severity};

/// Load and check `src`. Document rules run only when the file loaded without errors.
pub fn validate(src: &str) -> Vec<Diagnostic> {
    let (doc, mut diags) = crate::load::load(src);
    if !diags.iter().any(|d| d.severity() == Severity::Error) {
        diags.extend(crate::check::check(&doc));
    }
    diags.sort_by_key(|d| (d.line, d.col));
    diags
}

pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity() == Severity::Error)
}

/// Whether `graph` and `context` can still draw a file with these errors: only errors that
/// leave the design complete but unsafe, a table with no rule (SVT001) or money as `float`
/// (SVT005). An agent then sees the design while it fixes them.
pub fn drawable(diags: &[Diagnostic]) -> bool {
    diags
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .all(|d| matches!(d.code, Code::Svt001 | Code::Svt005))
}

fn severity(d: &Diagnostic) -> &'static str {
    match d.severity() {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

pub fn human(file: &str, diags: &[Diagnostic]) -> String {
    let mut out = String::new();
    for d in diags {
        out += &format!(
            "{file}:{}:{}: {} {}: {}\n  hint: {}\n",
            d.line,
            d.col + 1,
            severity(d),
            d.code.id(),
            d.message,
            d.hint
        );
    }
    let count = |s: Severity| diags.iter().filter(|d| d.severity() == s).count();
    let n = |k: usize, word: &str| format!("{k} {word}{}", if k == 1 { "" } else { "s" });
    let (e, w) = (count(Severity::Error), count(Severity::Warning));
    out += &if e == 0 {
        format!("{file} is valid ({}).\n", n(w, "warning"))
    } else {
        format!("{file}: {}, {}.\n", n(e, "error"), n(w, "warning"))
    };
    out
}

pub fn json(file: &str, diags: &[Diagnostic]) -> String {
    let items: Vec<String> = diags
        .iter()
        .map(|d| {
            format!(
                "{{\"code\":{},\"category\":{},\"severity\":{},\"path\":{},\"line\":{},\"column\":{},\"message\":{},\"retryable\":false,\"remediation\":{{\"action\":{},\"hint\":{}}}}}",
                q(d.code.id()),
                q(d.code.category()),
                q(severity(d)),
                q(&d.path),
                d.line,
                d.col + 1,
                q(&d.message),
                q(d.code.action()),
                q(&d.hint)
            )
        })
        .collect();
    format!(
        "{{\"file\":{},\"valid\":{},\"diagnostics\":[{}]}}\n",
        q(file),
        !has_errors(diags),
        items.join(",")
    )
}

/// A JSON string literal.
fn q(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            '\r' => out += "\\r",
            '\t' => out += "\\t",
            c if (c as u32) < 0x20 => out += &format!("\\u{:04x}", c as u32),
            c => out.push(c),
        }
    }
    out + "\""
}

#[cfg(test)]
mod tests {
    use super::q;

    #[test]
    fn escapes_json_strings() {
        assert_eq!(q("a\"b\\c\nd\u{1}"), r#""a\"b\\c\nd\u0001""#);
    }
}
