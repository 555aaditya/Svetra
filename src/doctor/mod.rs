//! `svetra doctor`: the pre-launch checklist (docs/doctor.md).
//!
//! Folder checks (leaked keys, the generated-files drift check, `schema_not_in_git`) plus, with a
//! database source, `check`'s findings under the area `data`. Nothing here touches the network,
//! writes a file, or prints a secret's value: a finding names a file, a line and a kind of key.

pub mod backups;
pub mod files;
pub mod generated;
pub mod jwt;
pub mod keys;

use crate::inspect::{self, CheckError, Source};
use crate::migrations::Outcome;
use crate::project;
use serde_json::{Value, json};
use std::path::Path;

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
    /// `data`, `keys`, `backups` or `generated`.
    pub area: &'static str,
    pub message: String,
    pub fix: String,
    /// The file, relative to the project folder; for a `data` finding, the database object.
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
    /// What the database source was, for the `Database:` line; `None` when none was given.
    pub database: Option<String>,
    /// Set when the source was migrations.
    pub migrations: Option<Outcome>,
}

const AREAS: [(&str, &str); 4] = [
    ("data", "Data exposure"),
    ("keys", "Leaked keys"),
    ("backups", "Backups and recovery"),
    ("generated", "Generated files"),
];

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

    /// The CLI's exit code: `1` for a critical finding in either group, else `2` when the
    /// migrations couldn't be read (none applied, or any failed with `strict`), else `0`.
    pub fn exit_code(&self, strict: bool) -> u8 {
        if self.has_critical() {
            return 1;
        }
        match &self.migrations {
            Some(o) if o.applied == 0 || (strict && !o.failed.is_empty()) => 2,
            _ => 0,
        }
    }

    /// The plain-English report.
    pub fn human(&self) -> String {
        let (c, w, i) = (
            self.count(Severity::Critical),
            self.count(Severity::Warning),
            self.count(Severity::Info),
        );
        let mut out = format!(
            "svetra doctor: {c} critical, {w} {}, {i} {}{}\n",
            plural(w, "warning", "warnings"),
            plural(i, "note", "notes"),
            if self.database.is_none() {
                " (database not checked)"
            } else {
                ""
            },
        );
        match &self.database {
            Some(d) => out.push_str(&format!("  Database: {d}\n")),
            None => out.push_str("  Database: not checked (no --db, --local or --migrations)\n"),
        }
        out.push_str(&format!(
            "  Project:  {} (git: {}, svetra.yaml: {})\n",
            self.project,
            yes(self.git),
            yes(self.has_design)
        ));
        let failed = self.migrations.as_ref().map_or(&[][..], |o| &o.failed[..]);
        for (area, title) in AREAS {
            let found: Vec<&Finding> = self.findings.iter().filter(|f| f.area == area).collect();
            let show_failed = area == "data" && !failed.is_empty();
            if found.is_empty() && !show_failed {
                continue;
            }
            out.push_str(&format!("\n{title}\n"));
            for f in found {
                out.push_str(&format!("  {:<9} {}\n", f.severity.name(), f.object()));
                out.push_str(&format!("            {}\n", f.message));
                out.push_str(&format!("            fix: {}\n", f.fix));
            }
            if show_failed {
                let n = failed.len();
                out.push_str(&format!(
                    "  {n} {} could not be applied, so the data findings may be incomplete:\n",
                    plural(n, "migration", "migrations")
                ));
                for (file, error) in failed {
                    out.push_str(&format!("    {file}  {error}\n"));
                }
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
                });
                if f.area != "data" {
                    v["file"] = json!(f.file);
                }
                if let Some(l) = f.line {
                    v["line"] = json!(l);
                }
                v
            })
            .collect();
        let mut v = json!({
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
        if let Some(o) = &self.migrations {
            v["migrations"] = json!({
                "files": o.files,
                "applied": o.applied,
                "failed": o.failed.iter().map(|(f, e)| json!({ "file": f, "error": e })).collect::<Vec<_>>(),
            });
        }
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

/// Backups can't be read from the folder: they live in the hosted project. A reminder, not a
/// finding, printed when the project looks hosted (docs/doctor.md, Backups and recovery).
const REMINDERS: [(&str, &str); 3] = [
    (
        "backups",
        "Backups: open Supabase, Project Settings, Database, Backups. Check that backups are on, how many days they are kept, and whether point-in-time recovery is enabled. Doctor can't see this.",
    ),
    (
        "restore",
        "Restore: a backup you have never restored is a hope. Restore one into a new project once.",
    ),
    (
        "own_copy",
        "Your own copy: a periodic `pg_dump`, kept somewhere that isn't your Supabase account, covers the case where the account itself is lost.",
    ),
];

/// Reads the project folder `dir` and, with a `source`, the database, and reports. The database
/// half is `inspect::run`, whose findings come in unchanged under the area `data`.
pub fn run(dir: &Path, source: Option<&Source>, schemas: &[String]) -> Result<Report, CheckError> {
    if !dir.is_dir() {
        return Err(CheckError::Usage(format!(
            "{} is not a folder",
            dir.display()
        )));
    }
    let checked = source.map(|s| inspect::run(s, schemas)).transpose()?;
    let files = files::walk(dir);
    let mut git = files::Git::open(dir);
    if let Some(g) = git.as_mut() {
        let candidates: Vec<&str> = files
            .iter()
            .map(|f| f.path.as_str())
            .filter(|p| keys::is_env_file(p) || *p == project::CONFIG)
            .collect();
        g.check_ignored(dir, &candidates);
    }
    let mut report = Report {
        project: dir.display().to_string(),
        git: git.is_some(),
        ..Report::default()
    };
    if let Some(check) = checked {
        report.database = Some(database_line(&check));
        let o = check.migrations.as_ref();
        if let Some(o) = o.filter(|o| o.applied == 0 || !o.failed.is_empty()) {
            let why = if o.applied == 0 {
                "none of the migrations could be applied, so no database was read".to_string()
            } else {
                format!(
                    "{} of {} migrations could not be applied, so the data findings may be incomplete",
                    o.failed.len(),
                    o.files
                )
            };
            report.not_checked.push(("Database", why));
        }
        match git.as_ref().filter(|_| check.has_tables) {
            Some(g) => report.findings.extend(backups::schema_not_in_git(dir, g)),
            None if git.is_none() => report.not_checked.push((
                "Schema in git",
                "this folder isn't a git repository, so doctor can't tell whether the schema is committed".to_string(),
            )),
            None => {}
        }
        report
            .findings
            .extend(check.findings.iter().map(|f| Finding {
                kind: f.kind,
                severity: match f.severity {
                    inspect::Severity::Critical => Severity::Critical,
                    inspect::Severity::Warning => Severity::Warning,
                    inspect::Severity::Info => Severity::Info,
                },
                area: "data",
                message: f.message.clone(),
                fix: f.fix.clone(),
                file: f.object.clone(),
                line: None,
            }));
        report.migrations = check.migrations;
    }
    let data = report.findings.len();
    report.findings.extend(keys::check(&files, git.as_ref()));
    match generated::check(dir) {
        Some(found) => {
            report.has_design = true;
            report.findings.extend(found);
        }
        None => report.not_checked.push((
            "Generated files",
            "there is no svetra.yaml in this folder, so there is nothing to compare them with"
                .to_string(),
        )),
    }
    // `check`'s findings keep their order; the folder's are sorted, so the output is stable.
    report.findings[data..].sort_by(|a, b| {
        (a.area, a.severity, &a.file, a.line, a.kind)
            .cmp(&(b.area, b.severity, &b.file, b.line, b.kind))
    });
    if dir.join("supabase").is_dir() || report.database.is_some() {
        report.reminders = REMINDERS
            .iter()
            .map(|(id, m)| (*id, m.to_string()))
            .collect();
    }
    if git.is_none() {
        report.not_checked.push((
            "Git",
            "this folder isn't a git repository, so doctor can't tell which files are committed and treats every secret as if its file were".to_string(),
        ));
    }
    report.not_checked.push((
        "Auth settings",
        "this version of doctor doesn't read supabase/config.toml's sign-in settings".to_string(),
    ));
    report.not_checked.push((
        "Storage buckets",
        "this version of doctor doesn't read storage.buckets".to_string(),
    ));
    if report.database.is_none() {
        report.not_checked.push((
            "Database",
            "no database was read: data exposure is not checked".to_string(),
        ));
    }
    Ok(report)
}

/// The `Database:` line: what was read, and how.
fn database_line(check: &inspect::Report) -> String {
    match &check.migrations {
        Some(o) => format!(
            "{} {}, {} applied (reads your migrations, not your live database)",
            o.files,
            plural(o.files, "migration", "migrations"),
            o.applied
        ),
        None => check.source.clone(),
    }
}
