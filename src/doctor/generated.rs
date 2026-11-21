//! The generated-files drift check (docs/doctor.md, Generated files): what
//! `svetra compile --check` finds, as findings. Never writes.

use super::{Finding, Severity};
use crate::diag::Severity as DiagSeverity;
use crate::project::{self, Failure};
use std::path::Path;

const DESIGN: &str = "svetra.yaml";

/// `None` when `dir` has no `svetra.yaml`: most projects don't, and the check doesn't apply.
pub fn check(dir: &Path) -> Option<Vec<Finding>> {
    let design = dir.join(DESIGN);
    if !design.is_file() {
        return None;
    }
    Some(match project::compile(&design, true) {
        Ok(run) => run.stale.iter().map(|entry| stale(entry)).collect(),
        Err(Failure::Invalid(diags)) => {
            let errors = diags
                .iter()
                .filter(|d| d.severity() == DiagSeverity::Error)
                .count();
            vec![invalid(format!(
                "{DESIGN} has {errors} {}, so doctor couldn't check that your generated files are current.",
                if errors == 1 { "error" } else { "errors" }
            ))]
        }
        Err(Failure::HoleBodies(bodies)) => vec![invalid(format!(
            "{} hole {} would make the generated SQL invalid, so doctor couldn't check that your generated files are current.",
            bodies.len(),
            if bodies.len() == 1 { "body" } else { "bodies" }
        ))],
        Err(Failure::Read(_) | Failure::Write(_)) => vec![invalid(format!(
            "{DESIGN} couldn't be read, so doctor couldn't check that your generated files are current."
        ))],
    })
}

/// One entry of `compile --check`'s `stale`: a file, or `supabase/config.toml (<key> is …)`.
fn stale(entry: &str) -> Finding {
    let (file, detail) = match entry.split_once(" (") {
        Some((file, rest)) => (file, Some(rest.trim_end_matches(')'))),
        None => (entry, None),
    };
    let message = match detail {
        Some(d) => format!(
            "{file} differs from {DESIGN}: {d}. The sign-in settings that run may not be the ones you designed."
        ),
        None => format!(
            "{file} no longer matches {DESIGN} (it was hand-edited, is missing, or the design changed). The rules in your database may not be the rules you designed."
        ),
    };
    Finding {
        kind: "stale_generated",
        severity: Severity::Warning,
        area: "generated",
        message,
        fix: "run `svetra compile`.".to_string(),
        file: file.to_string(),
        line: None,
    }
}

fn invalid(message: String) -> Finding {
    Finding {
        kind: "design_invalid",
        severity: Severity::Warning,
        area: "generated",
        message,
        fix: "run `svetra validate` and fix what it reports.".to_string(),
        file: DESIGN.to_string(),
        line: None,
    }
}
