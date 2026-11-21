//! `svetra doctor`: the pre-launch checklist (docs/doctor.md).
//!
//! This build reads the project folder only: leaked keys and the generated-files drift check.
//! Nothing here touches the network, writes a file, or prints a secret's value: a finding names
//! a file, a line and a kind of key.

pub mod jwt;

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Critical => "critical",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// One thing doctor found in the project folder.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Stable id, such as `service_key_exposed`.
    pub kind: &'static str,
    pub severity: Severity,
    /// `keys` or `generated`.
    pub area: &'static str,
    pub message: String,
    pub fix: String,
    /// The file, relative to the project folder.
    pub file: String,
    /// The line in `file`, when the finding is about one.
    pub line: Option<usize>,
}

impl Finding {
    /// `file:line`, or `file`.
    pub fn object(&self) -> String {
        match self.line {
            Some(l) => format!("{}:{l}", self.file),
            None => self.file.clone(),
        }
    }
}

/// What doctor read and found. `findings` is sorted, so the output is deterministic.
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// Things doctor can't read and you should do: `(id, message)`.
    pub reminders: Vec<(&'static str, String)>,
    /// Parts that were skipped: `(area, why)`.
    pub not_checked: Vec<(&'static str, String)>,
    /// The project folder, as given.
    pub project: String,
    pub git: bool,
    pub has_design: bool,
}

const AREAS: [(&str, &str); 2] = [("keys", "Leaked keys"), ("generated", "Generated files")];

impl Report {
    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }

    /// True only for a clean run in which nothing was skipped (docs/doctor.md, Output).
    pub fn ready(&self) -> bool {
        self.count(Severity::Critical) == 0 && self.not_checked.is_empty()
    }

    pub fn has_critical(&self) -> bool {
        self.count(Severity::Critical) > 0
    }

    /// The plain-English report.
    pub fn human(&self) -> String {
        let (c, w, i) = (
            self.count(Severity::Critical),
            self.count(Severity::Warning),
            self.count(Severity::Info),
        );
        let mut out = format!(
            "svetra doctor: {c} critical, {w} {}, {i} {} (database not checked)\n",
            plural(w, "warning", "warnings"),
            plural(i, "note", "notes"),
        );
        out.push_str("  Database: not checked (this version reads the project folder only)\n");
        out.push_str(&format!(
            "  Project:  {} (git: {}, svetra.yaml: {})\n",
            self.project,
            yes(self.git),
            yes(self.has_design)
        ));
        for (area, title) in AREAS {
            let found: Vec<&Finding> = self.findings.iter().filter(|f| f.area == area).collect();
            if found.is_empty() {
                continue;
            }
            out.push_str(&format!("\n{title}\n"));
            for f in found {
                out.push_str(&format!("  {:<9} {}\n", f.severity.name(), f.object()));
                out.push_str(&format!("            {}\n", f.message));
                out.push_str(&format!("            fix: {}\n", f.fix));
            }
        }
        if !self.reminders.is_empty() {
            out.push_str("\nReminders (doctor can't check these)\n");
            for (_, message) in &self.reminders {
                out.push_str(&format!("  - {message}\n"));
            }
        }
        if !self.not_checked.is_empty() {
            out.push_str("\nNot checked\n");
            for (area, why) in &self.not_checked {
                out.push_str(&format!("  - {area}: {why}\n"));
            }
        }
        out.push_str(&format!("\nBefore launch: {}\n", self.verdict()));
        out
    }

    /// The last line: one of three, and only the third calls the project clear.
    fn verdict(&self) -> String {
        let critical = self.count(Severity::Critical);
        if critical > 0 {
            format!(
                "fix the {critical} critical {} first. Not ready.",
                plural(critical, "finding", "findings")
            )
        } else if !self.not_checked.is_empty() {
            let n = self.not_checked.len();
            format!(
                "no critical findings, but {n} {} not checked (listed above). Not a clearance.",
                plural(n, "part was", "parts were")
            )
        } else {
            "no critical findings in everything doctor can check. The reminders above are yours."
                .to_string()
        }
    }

    /// The JSON report (docs/doctor.md, Findings and output).
    pub fn json(&self) -> String {
        let findings: Vec<Value> = self
            .findings
            .iter()
            .map(|f| {
                let mut v = json!({
                    "kind": f.kind,
                    "severity": f.severity.name(),
                    "area": f.area,
                    "object": f.object(),
                    "message": f.message,
                    "fix": f.fix,
                    "file": f.file,
                });
                if let Some(l) = f.line {
                    v["line"] = json!(l);
                }
                v
            })
            .collect();
        let v = json!({
            "findings": findings,
            "counts": {
                "critical": self.count(Severity::Critical),
                "warning": self.count(Severity::Warning),
                "info": self.count(Severity::Info),
            },
            "reminders": self.reminders.iter()
                .map(|(id, message)| json!({ "id": id, "message": message }))
                .collect::<Vec<_>>(),
            "not_checked": self.not_checked.iter()
                .map(|(area, why)| json!({ "area": area, "why": why }))
                .collect::<Vec<_>>(),
            "ready": self.ready(),
        });
        let mut s = serde_json::to_string_pretty(&v).unwrap_or_default();
        s.push('\n');
        s
    }
}

fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
