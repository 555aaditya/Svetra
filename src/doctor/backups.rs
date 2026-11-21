//! The one backups check doctor can make from the folder (docs/doctor.md, Backups and recovery).

use super::{Finding, Severity};
use crate::doctor::files::Git;
use std::path::Path;

/// `schema_not_in_git`: the database has tables, but no `.sql` file under `supabase/migrations/`
/// or `supabase/schemas/` is tracked. Only for a project with a `supabase/` folder and git.
pub fn schema_not_in_git(dir: &Path, git: &Git) -> Option<Finding> {
    if !dir.join("supabase").is_dir()
        || git.tracks_sql_under(&["supabase/migrations/", "supabase/schemas/"])
    {
        return None;
    }
    Some(Finding {
        kind: "schema_not_in_git",
        severity: Severity::Warning,
        area: "backups",
        message: "Your tables exist only in the live database. If the project is lost, nothing can recreate them.".into(),
        fix: "`supabase db pull` writes them as a migration; commit it.".into(),
        file: "supabase".into(),
        line: None,
    })
}
