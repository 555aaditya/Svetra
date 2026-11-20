//! `svetra check --migrations` (docs/check.md, "Checking migrations"): build a project's migrations
//! in a throwaway database, then read its snapshot for the checks.
//!
//! By default the database is one PostgreSQL 16 container with a stand-in for Supabase, started
//! and removed here. With `--scratch URL` it is a database the caller provides, which must be empty.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The stand-in for Supabase: the base of the access tests' database, then what real migrations
/// also assume (the service role, the `storage` and `extensions` schemas, `auth.role()`).
const BASE: &str = include_str!("standin/base.sql");
const EXTRA: &str = include_str!("standin/extra.sql");
const IMAGE: &str = "postgres:16";
/// A migration file larger than this is reported as failed, not applied.
const MAX_FILE_BYTES: u64 = 2_000_000;
/// The container stops itself after this many seconds, so an interrupted run leaves nothing behind.
const LIFETIME_SECONDS: u32 = 900;

/// What applying the migrations did.
pub struct Outcome {
    pub files: usize,
    pub applied: usize,
    /// (file name, first line of the database's error), in file order.
    pub failed: Vec<(String, String)>,
    /// The snapshot JSON, as the check query returns it; empty when nothing applied.
    pub snapshot: String,
}

/// A failure that stops the check: the message is for the person, the exit code is always 2.
pub struct Fail(pub String);

/// Every `*.sql` file directly in `dir`, in file-name order. Symlinks are skipped.
pub fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|_| format!("{} isn't a folder of migrations", dir.display()))?;
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sql"))
        .collect();
    out.sort();
    if out.is_empty() {
        return Err(format!("{} holds no .sql file", dir.display()));
    }
    Ok(out)
}

/// The first line of the database's own error, without the SQL around it.
pub fn first_error(stderr: &str) -> String {
    for line in stderr.lines() {
        if let Some((_, rest)) = line.split_once("ERROR:") {
            return rest.trim().to_string();
        }
    }
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("the migration failed")
        .to_string()
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// A schema name that is safe to put in a query.
fn plain(schemas: &[String]) -> Result<(), String> {
    match schemas
        .iter()
        .find(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
    {
        Some(s) => Err(format!("'{s}' isn't a schema name")),
        None => Ok(()),
    }
}

/// Run `program args`, feeding `input` on stdin, and collect its output.
fn run_with(program: &str, args: &[String], input: &[u8]) -> std::io::Result<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped");
    let bytes = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    let out = child.wait_with_output();
    let _ = writer.join();
    out
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// The throwaway container, removed when this drops.
struct Container(String);

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl Container {
    fn psql(&self, extra: &[&str], input: &str) -> std::io::Result<Output> {
        let mut args = strings(&[
            "exec",
            "-i",
            "-e",
            "PGOPTIONS=-c statement_timeout=30000",
            &self.0,
            "psql",
            "-U",
            "postgres",
            "-d",
            "postgres",
            "-X",
            "-q",
        ]);
        args.extend(extra.iter().map(|s| s.to_string()));
        run_with("docker", &args, input.as_bytes())
    }
}

pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Build the migrations in a throwaway Docker database and read the snapshot.
pub fn run_docker(dir: &Path, query: &str) -> Result<Outcome, Fail> {
    let paths = files(dir).map_err(Fail)?;
    if !docker_available() {
        return Err(Fail(
            "Docker isn't running. Start Docker, or use --scratch with a throwaway database."
                .into(),
        ));
    }
    eprintln!(
        "svetra: building {} migrations in a throwaway PostgreSQL (Docker)…",
        paths.len()
    );
    let name = format!("svetra-check-{}", std::process::id());
    let started = Command::new("docker")
        .args([
            "run",
            "-d",
            "--rm",
            "--name",
            &name,
            "--network",
            "none",
            "--cap-drop",
            "ALL",
            "--cap-add",
            "CHOWN",
            "--cap-add",
            "DAC_OVERRIDE",
            "--cap-add",
            "FOWNER",
            "--cap-add",
            "SETGID",
            "--cap-add",
            "SETUID",
            "--security-opt",
            "no-new-privileges",
            "--memory",
            "1g",
            "--pids-limit",
            "512",
            "-e",
            "POSTGRES_PASSWORD=svetra",
            IMAGE,
            "timeout",
            &LIFETIME_SECONDS.to_string(),
            "docker-entrypoint.sh",
            "postgres",
        ])
        .output()
        .map_err(|e| Fail(format!("couldn't start Docker: {e}")))?;
    if !started.status.success() {
        return Err(Fail(format!(
            "Docker couldn't start the database: {}",
            String::from_utf8_lossy(&started.stderr).trim()
        )));
    }
    let container = Container(name);
    // The image restarts the server once after its first start; wait until it stays up.
    let mut steady = 0;
    for _ in 0..90 {
        let up = container
            .psql(&["-A", "-t", "-c", "select 1"], "")
            .is_ok_and(|o| o.status.success());
        steady = if up { steady + 1 } else { 0 };
        if steady >= 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    if steady < 3 {
        return Err(Fail("the throwaway database didn't start in time".into()));
    }
    for (what, sql) in [("base", BASE), ("extra", EXTRA)] {
        let out = container
            .psql(&["-v", "ON_ERROR_STOP=1"], sql)
            .map_err(|e| Fail(format!("couldn't load the Supabase stand-in: {e}")))?;
        if !out.status.success() {
            return Err(Fail(format!(
                "couldn't load the Supabase stand-in ({what}): {}",
                first_error(&String::from_utf8_lossy(&out.stderr))
            )));
        }
    }
    let mut failed = Vec::new();
    let mut applied = 0;
    for p in &paths {
        let file = name_of(p);
        if std::fs::metadata(p).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
            failed.push((file, "the file is larger than 2 MB".to_string()));
            continue;
        }
        let sql = std::fs::read(p).unwrap_or_default();
        let sql = String::from_utf8_lossy(&sql).to_string();
        match container.psql(&["-1", "-v", "ON_ERROR_STOP=1"], &sql) {
            Ok(o) if o.status.success() => applied += 1,
            Ok(o) => failed.push((file, first_error(&String::from_utf8_lossy(&o.stderr)))),
            Err(e) => return Err(Fail(format!("lost the throwaway database: {e}"))),
        }
    }
    let snapshot = if applied == 0 {
        String::new()
    } else {
        let out = container
            .psql(&["-A", "-t", "-v", "ON_ERROR_STOP=1", "-c", query], "")
            .map_err(|e| Fail(format!("lost the throwaway database: {e}")))?;
        if !out.status.success() {
            return Err(Fail(format!(
                "the database refused the check query: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    Ok(Outcome {
        files: paths.len(),
        applied,
        failed,
        snapshot,
    })
}

/// Build the migrations in a database the caller provides, which must be empty.
pub fn run_scratch(
    dir: &Path,
    url: &str,
    schemas: &[String],
    query: &str,
) -> Result<Outcome, Fail> {
    let paths = files(dir).map_err(Fail)?;
    plain(schemas).map_err(Fail)?;
    let psql = |extra: &[&str]| {
        let mut args = vec![url, "-X", "-A", "-t", "-q", "-v", "ON_ERROR_STOP=1"];
        args.extend(extra);
        Command::new("psql").args(args).output()
    };
    let list = schemas
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let count = format!(
        "select (select count(*) from pg_class c join pg_namespace n on n.oid = c.relnamespace \
         where n.nspname in ({list}) and c.relkind in ('r','p','v','m','f','S')) \
         + (select count(*) from pg_proc p join pg_namespace n on n.oid = p.pronamespace \
         where n.nspname in ({list}) and not exists \
         (select 1 from pg_depend d where d.objid = p.oid and d.deptype = 'e'))"
    );
    let empty = match psql(&["-c", &count]) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim() == "0",
        Ok(o) => {
            return Err(Fail(format!(
                "the database refused the query: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            )));
        }
        Err(_) => {
            return Err(Fail(
                "psql isn't installed, and --scratch needs it. Install psql, or use --migrations without --scratch (needs Docker)."
                    .into(),
            ));
        }
    };
    if !empty {
        return Err(Fail(format!(
            "that database isn't empty ({} has tables, views or functions). --scratch runs your migrations in it, so point it at a throwaway database, never at one with data.",
            schemas.join(", ")
        )));
    }
    eprintln!(
        "svetra: running {} migrations in {} (it will keep them)…",
        paths.len(),
        crate::inspect::redact(url)
    );
    let mut failed = Vec::new();
    let mut applied = 0;
    for p in &paths {
        let file = name_of(p);
        if std::fs::metadata(p).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
            failed.push((file, "the file is larger than 2 MB".to_string()));
            continue;
        }
        let out = Command::new("psql")
            .args([url, "-X", "-q", "-1", "-v", "ON_ERROR_STOP=1", "-f"])
            .arg(p)
            .output()
            .map_err(|e| Fail(format!("couldn't run psql: {e}")))?;
        if out.status.success() {
            applied += 1;
        } else {
            failed.push((file, first_error(&String::from_utf8_lossy(&out.stderr))));
        }
    }
    let snapshot = if applied == 0 {
        String::new()
    } else {
        match psql(&["-c", query]) {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
            Ok(o) => {
                return Err(Fail(format!(
                    "the database refused the check query: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                )));
            }
            Err(e) => return Err(Fail(format!("couldn't run psql: {e}"))),
        }
    };
    Ok(Outcome {
        files: paths.len(),
        applied,
        failed,
        snapshot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_error_is_the_databases_line_without_the_sql() {
        let stderr = "psql:/dev/stdin:3: ERROR:  relation \"public.legacy_orders\" does not exist\nLINE 1: select * from public.legacy_orders\n                      ^\n";
        assert_eq!(
            first_error(stderr),
            "relation \"public.legacy_orders\" does not exist"
        );
        assert_eq!(first_error(""), "the migration failed");
        assert_eq!(
            first_error("psql: could not connect\n"),
            "psql: could not connect"
        );
    }

    #[test]
    fn schema_names_are_plain() {
        assert!(plain(&["public".into(), "api_v2".into()]).is_ok());
        assert!(plain(&["pub'lic".into()]).is_err());
        assert!(plain(&["".into()]).is_err());
    }

    #[test]
    fn migrations_run_in_file_name_order_and_only_sql() {
        let dir = std::env::temp_dir().join(format!("svetra-mig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "20260102_b.sql",
            "20260101_a.sql",
            "notes.txt",
            "20260103_c.sql",
        ] {
            std::fs::write(dir.join(f), "select 1;").unwrap();
        }
        let names: Vec<String> = files(&dir).unwrap().iter().map(|p| name_of(p)).collect();
        assert_eq!(
            names,
            ["20260101_a.sql", "20260102_b.sql", "20260103_c.sql"]
        );
        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(files(&empty).unwrap_err().contains("no .sql file"));
        assert!(
            files(&dir.join("missing"))
                .unwrap_err()
                .contains("isn't a folder")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
