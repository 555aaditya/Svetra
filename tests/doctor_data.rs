//! `svetra doctor`'s database half (docs/doctor.md): `check`'s findings under `data`, and
//! `schema_not_in_git`. The runs that start a throwaway database need Docker and are skipped
//! without it. Projects are built at run time with a real `git init`.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn svetra(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(args)
        .output()
        .unwrap()
}

fn docker_up() -> bool {
    svetra::migrations::docker_available()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(format!(
        "{}/tests/fixtures/check/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

/// A project folder in a fresh temporary directory, with a real `git init`.
struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Project {
        let d =
            std::env::temp_dir().join(format!("svetra-doctor-data-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        git(&d, &["init", "-q"]);
        Project(d)
    }

    fn write(&self, path: &str, text: &str) -> &Project {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
        self
    }

    /// Copies a `check` fixture into `supabase/migrations`.
    fn migrations(&self, fixture_name: &str) -> &Project {
        let to = self.0.join("supabase/migrations");
        std::fs::create_dir_all(&to).unwrap();
        for e in std::fs::read_dir(fixture(fixture_name)).unwrap() {
            let e = e.unwrap();
            std::fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
        self
    }

    fn track(&self, path: &str) -> &Project {
        git(&self.0, &["add", "-f", path]);
        self
    }

    fn json(&self, extra: &[&str]) -> (i32, Value) {
        let mut args = vec!["doctor", self.0.to_str().unwrap(), "--json"];
        args.extend_from_slice(extra);
        let out = svetra(&args);
        (
            out.status.code().unwrap(),
            serde_json::from_slice(&out.stdout).unwrap_or_default(),
        )
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn kinds(v: &Value, area: &str) -> Vec<String> {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["area"] == area)
        .map(|f| f["kind"].as_str().unwrap().to_string())
        .collect()
}

/// A JWT-shaped service key, assembled from pieces so nothing key-like is committed.
fn service_key() -> String {
    // {"role":"service_role"} and a header, base64url without padding.
    let header = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
    let payload = "eyJyb2xlIjoic2VydmljZV9yb2xlIn0";
    format!("{header}.{payload}.ZHVtbXktc2lnbmF0dXJl")
}

#[test]
fn the_leaky_app_through_doctor_gives_checks_findings_under_data() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let p = Project::new("leaky");
    p.migrations("migrations");
    p.track("supabase/migrations");
    let (code, v) = p.json(&["--migrations"]);
    assert_eq!(code, 1, "critical findings exit 1");
    let golden: Value = serde_json::from_str(include_str!("golden/leaky_app.check.json")).unwrap();
    let data: Vec<Value> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["area"] == "data")
        .map(|f| {
            let mut f = f.clone();
            f.as_object_mut().unwrap().remove("area");
            f
        })
        .collect();
    assert_eq!(Value::Array(data), golden["findings"]);
    assert_eq!(
        v["migrations"],
        serde_json::json!({ "files": 3, "applied": 3, "failed": [] })
    );
    assert!(kinds(&v, "backups").is_empty(), "migrations are tracked");
    assert_eq!(v["ready"], false);
    let human =
        String::from_utf8(svetra(&["doctor", p.0.to_str().unwrap(), "--migrations"]).stdout)
            .unwrap();
    assert!(human.contains("Data exposure"), "{human}");
    assert!(
        human.contains("Database: 3 migrations, 3 applied"),
        "{human}"
    );
    assert!(!human.contains("database not checked"), "{human}");
    assert!(!human.contains("Database: not checked"), "{human}");
    assert!(
        human.lines().last().unwrap().contains("Not ready."),
        "{human}"
    );
}

#[test]
fn a_committed_service_key_and_leaky_migrations_give_both_groups() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let p = Project::new("both");
    p.migrations("migrations");
    p.write(
        "src/lib/supabase.ts",
        &format!("const key = \"{}\";\n", service_key()),
    );
    let (code, v) = p.json(&["--migrations"]);
    assert_eq!(code, 1);
    assert!(kinds(&v, "data").contains(&"open_write".to_string()), "{v}");
    assert_eq!(kinds(&v, "keys"), ["service_key_exposed"], "{v}");
    assert_eq!(
        kinds(&v, "backups"),
        ["schema_not_in_git"],
        "migrations exist but aren't tracked: {v}"
    );
    let d = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["area"] == "data")
        .unwrap();
    assert!(d.get("file").is_none(), "{d}");
    assert!(!v.to_string().contains(&service_key()));
}

#[test]
fn tracked_migrations_give_no_schema_not_in_git_and_untracked_give_it() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let p = Project::new("tracked");
    p.migrations("migrations");
    let (_, v) = p.json(&["--migrations"]);
    assert_eq!(kinds(&v, "backups"), ["schema_not_in_git"], "{v}");
    // The migrations are on disk, only not tracked: the message says so, not "only in the database".
    let finding = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["kind"] == "schema_not_in_git")
        .unwrap();
    assert!(
        finding["message"]
            .as_str()
            .unwrap()
            .contains("doesn't track"),
        "{v}"
    );
    assert!(
        finding["fix"]
            .as_str()
            .unwrap()
            .contains("git add supabase"),
        "{v}"
    );
    p.track("supabase/migrations");
    let (_, v) = p.json(&["--migrations"]);
    assert!(kinds(&v, "backups").is_empty(), "{v}");
}

#[test]
fn a_broken_migration_makes_the_database_a_part_not_checked() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let p = Project::new("broken");
    p.migrations("migrations_broken");
    p.track("supabase/migrations");
    let (code, v) = p.json(&["--migrations"]);
    assert_eq!(code, 1, "the leaks are still found");
    assert_eq!(v["migrations"]["failed"].as_array().unwrap().len(), 1);
    assert!(v["not_checked"].to_string().contains("Database"), "{v}");
    let human =
        String::from_utf8(svetra(&["doctor", p.0.to_str().unwrap(), "--migrations"]).stdout)
            .unwrap();
    assert!(
        human.contains("1 migration could not be applied, so the data findings may be incomplete"),
        "{human}"
    );
    assert!(human.contains("20260102120000_reports.sql"), "{human}");
}

#[test]
fn strict_makes_a_failed_migration_exit_2_without_a_critical_finding() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let p = Project::new("strict");
    p.write(
        "supabase/migrations/20260101000000_ok.sql",
        "create table public.notes (id int primary key);\nalter table public.notes enable row level security;\n",
    );
    p.write(
        "supabase/migrations/20260102000000_bad.sql",
        "select nope;\n",
    );
    p.track("supabase/migrations");
    assert_eq!(p.json(&["--migrations"]).0, 0);
    assert_eq!(p.json(&["--migrations", "--strict"]).0, 2);
}

#[test]
fn a_clean_database_is_still_never_called_ready() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let p = Project::new("clean");
    p.write(
        "supabase/migrations/20260101000000_ok.sql",
        "create table public.notes (id int primary key);\nalter table public.notes enable row level security;\n",
    );
    p.track("supabase/migrations");
    let (code, v) = p.json(&["--migrations"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["ready"], false, "no config.toml and no svetra.yaml");
    assert!(
        v["not_checked"].to_string().contains("Auth settings"),
        "{v}"
    );
    assert!(
        !v["not_checked"].to_string().contains("\"Database\""),
        "{v}"
    );
}

#[test]
fn a_missing_migrations_folder_or_unreachable_database_exits_2() {
    let p = Project::new("missing");
    let out = svetra(&["doctor", p.0.to_str().unwrap(), "--migrations"]);
    assert_eq!(out.status.code(), Some(2));
    let out = svetra(&[
        "doctor",
        p.0.to_str().unwrap(),
        "--db",
        "postgresql://nobody:secretpw@127.0.0.1:1/none",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("secretpw"),
        "no password in errors"
    );
}

#[test]
fn source_options_are_checked_before_anything_runs() {
    let p = Project::new("usage");
    let dir = p.0.to_str().unwrap();
    for args in [
        vec!["doctor", dir, "--strict"],
        vec!["doctor", dir, "--scratch", "postgresql://x@h/d"],
        vec!["doctor", dir, "--db", "postgresql://x@h/d", "--local"],
        vec!["doctor", dir, "--local", "--migrations"],
        vec!["doctor", dir, "--db"],
    ] {
        assert_eq!(svetra(&args).status.code(), Some(2), "{args:?}");
    }
}
