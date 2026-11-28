//! The one backups check doctor can make from the folder (docs/doctor.md, Backups and recovery).

use super::{Finding, Severity};
use crate::doctor::files::Git;
use std::path::Path;

/// `schema_not_in_git`: the database has tables, but no `.sql` file under `supabase/migrations/`
/// or `supabase/schemas/` is tracked. Only for a project with a `supabase/` folder and git.
///
/// The wording depends on what is on disk: files that git doesn't track yet need a `git add`;
/// no files at all means the tables exist only in the database.
pub fn schema_not_in_git(dir: &Path, git: &Git) -> Option<Finding> {
    if !dir.join("supabase").is_dir()
        || git.tracks_sql_under(&["supabase/migrations/", "supabase/schemas/"])
    {
        return None;
    }
    let (message, fix) = wording(sql_on_disk(dir));
    Some(Finding {
        kind: "schema_not_in_git",
        severity: Severity::Warning,
        area: "backups",
        message,
        fix,
        file: "supabase".into(),
        line: None,
    })
}

/// The folder holding `.sql` files that exist on disk (migrations first), if any.
fn sql_on_disk(dir: &Path) -> Option<&'static str> {
    ["supabase/migrations", "supabase/schemas"]
        .into_iter()
        .find(|d| {
            std::fs::read_dir(dir.join(d)).is_ok_and(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| e.path().extension().is_some_and(|x| x == "sql"))
            })
        })
}

/// The message and the fix for `schema_not_in_git`, given where `.sql` files are on disk.
fn wording(on_disk: Option<&str>) -> (String, String) {
    match on_disk {
        Some(folder) => (
            format!("The files in {folder} describe your tables, but git doesn't track them yet. If this folder is lost, so are they."),
            "`git add supabase`, then commit.".into(),
        ),
        None => (
            "Your tables exist only in the live database. If the project is lost, nothing can recreate them.".into(),
            "`supabase db pull` writes them as a migration; commit it.".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untracked_files_say_to_add_them_and_no_files_say_to_pull() {
        let (m, f) = wording(Some("supabase/migrations"));
        assert!(
            m.contains("supabase/migrations") && m.contains("doesn't track"),
            "{m}"
        );
        assert!(f.contains("git add supabase"), "{f}");
        let (m, f) = wording(None);
        assert!(m.contains("only in the live database"), "{m}");
        assert!(f.contains("supabase db pull"), "{f}");
    }

    #[test]
    fn sql_on_disk_prefers_migrations_and_ignores_other_files() {
        let d = std::env::temp_dir().join(format!("svetra-backups-{}", std::process::id()));
        std::fs::create_dir_all(d.join("supabase/migrations")).unwrap();
        std::fs::create_dir_all(d.join("supabase/schemas")).unwrap();
        assert_eq!(sql_on_disk(&d), None);
        std::fs::write(d.join("supabase/migrations/readme.txt"), "x").unwrap();
        assert_eq!(sql_on_disk(&d), None);
        std::fs::write(d.join("supabase/schemas/a.sql"), "select 1;").unwrap();
        assert_eq!(sql_on_disk(&d), Some("supabase/schemas"));
        std::fs::write(d.join("supabase/migrations/1.sql"), "select 1;").unwrap();
        assert_eq!(sql_on_disk(&d), Some("supabase/migrations"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}
