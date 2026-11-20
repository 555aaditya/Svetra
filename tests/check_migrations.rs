//! `svetra check --migrations` (docs/check.md). The runs that start a throwaway database need
//! Docker and are skipped without it; the rest run anywhere.
use serde_json::Value;
use std::path::PathBuf;
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

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/check/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("svetra-check-mig-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_leaky_app_split_into_migrations_gives_the_same_findings() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let out = svetra(&["check", "--migrations", &fixture("migrations"), "--json"]);
    assert_eq!(out.status.code(), Some(1), "critical findings exit 1");
    let mut got: Value = serde_json::from_slice(&out.stdout).unwrap();
    let migrations = got.as_object_mut().unwrap().remove("migrations").unwrap();
    assert_eq!(
        migrations,
        serde_json::json!({ "files": 3, "applied": 3, "failed": [] })
    );
    let golden: Value = serde_json::from_str(include_str!("golden/leaky_app.check.json")).unwrap();
    assert_eq!(got, golden);
}

#[test]
fn a_failing_migration_is_listed_and_the_rest_still_run() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let out = svetra(&["check", "--migrations", &fixture("migrations_broken")]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "the leaks are still found");
    assert!(text.contains("(4 migrations, 3 applied)"), "{text}");
    assert!(
        text.contains("1 migration could not be applied, so the findings above may be incomplete"),
        "{text}"
    );
    assert!(
        text.contains(
            "20260102120000_reports.sql  relation \"public.legacy_orders\" does not exist"
        ),
        "{text}"
    );
    assert!(!text.contains("create policy"), "never the SQL: {text}");
    assert!(text.contains("This reads your migrations, not your live database."));
    let json = svetra(&[
        "check",
        "--migrations",
        &fixture("migrations_broken"),
        "--json",
    ]);
    let v: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(v["migrations"]["files"], 4);
    assert_eq!(v["migrations"]["applied"], 3);
    assert_eq!(
        v["migrations"]["failed"][0]["file"],
        "20260102120000_reports.sql"
    );
}

#[test]
fn strict_exits_two_when_any_migration_failed_and_nothing_critical_was_found() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let dir = temp("strict");
    std::fs::write(
        dir.join("20260101_a.sql"),
        "create table public.t (id int); alter table public.t enable row level security;",
    )
    .unwrap();
    std::fs::write(dir.join("20260102_b.sql"), "select * from nowhere;").unwrap();
    let d = dir.to_str().unwrap();
    assert_eq!(svetra(&["check", "--migrations", d]).status.code(), Some(0));
    assert_eq!(
        svetra(&["check", "--migrations", d, "--strict"])
            .status
            .code(),
        Some(2)
    );
    // Nothing applied: nothing was checked, so it is never reported as clean.
    let only_bad = temp("bad");
    std::fs::write(only_bad.join("20260101_a.sql"), "select * from nowhere;").unwrap();
    let out = svetra(&["check", "--migrations", only_bad.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("the migration could not be applied, so nothing was checked"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(only_bad);
}

#[test]
fn without_docker_it_says_so_in_plain_words() {
    let nowhere = temp("nopath");
    let out = Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(["check", "--migrations", &fixture("migrations")])
        .env("PATH", &nowhere)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(
            "Docker isn't running. Start Docker, or use --scratch with a throwaway database."
        ),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(nowhere);
}

#[test]
fn usage_errors_exit_two_without_touching_docker() {
    let missing = svetra(&["check", "--migrations", "/no/such/folder"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("isn't a folder of migrations"));
    let empty = temp("empty");
    let none = svetra(&["check", "--migrations", empty.to_str().unwrap()]);
    assert_eq!(none.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&none.stderr).contains("holds no .sql file"));
    assert_eq!(
        svetra(&["check", "--migrations", "--db", "x"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        svetra(&["check", "--migrations", "--snapshot", "x"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(svetra(&["check", "--scratch", "x"]).status.code(), Some(2));
    assert_eq!(svetra(&["check", "--strict"]).status.code(), Some(2));
    let _ = std::fs::remove_dir_all(empty);
}

/// A Supabase-like database in Docker that `--scratch` can use, removed when dropped.
struct Scratch {
    name: String,
    url: String,
}

impl Scratch {
    fn start() -> Option<Scratch> {
        let psql_ok = Command::new("psql").arg("--version").output().is_ok();
        if !svetra::migrations::docker_available() || !psql_ok {
            return None;
        }
        let name = format!("svetra-scratch-test-{}", std::process::id());
        let run = Command::new("docker")
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                &name,
                "-p",
                "127.0.0.1::5432",
            ])
            .args(["-e", "POSTGRES_PASSWORD=pw", "postgres:16"])
            .output()
            .ok()?;
        let mut scratch = Scratch {
            name: name.clone(),
            url: String::new(),
        };
        if !run.status.success() {
            return None;
        }
        let port = loop_until(|| {
            let o = Command::new("docker")
                .args(["port", &name, "5432"])
                .output()
                .ok()?;
            let t = String::from_utf8_lossy(&o.stdout).to_string();
            t.lines().next()?.rsplit(':').next().map(str::to_string)
        })?;
        let url = format!("postgresql://postgres:pw@127.0.0.1:{port}/postgres");
        let mut steady = 0;
        for _ in 0..90 {
            let up = Command::new("psql")
                .args([url.as_str(), "-Atc", "select 1"])
                .output()
                .is_ok_and(|o| o.status.success());
            steady = if up { steady + 1 } else { 0 };
            if steady >= 3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        for f in ["base.sql", "extra.sql"] {
            let path = format!("{}/src/standin/{f}", env!("CARGO_MANIFEST_DIR"));
            let out = Command::new("psql")
                .args([
                    url.as_str(),
                    "-X",
                    "-q",
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-f",
                    &path,
                ])
                .output()
                .ok()?;
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        scratch.url = url;
        Some(scratch)
    }
}

fn loop_until<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
    for _ in 0..30 {
        if let Some(v) = f() {
            return Some(v);
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    None
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.name])
            .output();
    }
}

#[test]
fn scratch_uses_an_empty_database_and_refuses_one_that_is_not() {
    let Some(db) = Scratch::start() else {
        eprintln!("skipped: needs Docker and psql");
        return;
    };
    let args = [
        "check",
        "--migrations",
        &fixture("migrations"),
        "--scratch",
        &db.url,
        "--json",
    ];
    let out = svetra(&args);
    assert_eq!(out.status.code(), Some(1));
    let mut got: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(got["migrations"]["applied"], 3);
    got.as_object_mut().unwrap().remove("migrations");
    let golden: Value = serde_json::from_str(include_str!("golden/leaky_app.check.json")).unwrap();
    assert_eq!(got, golden);
    // The migrations are still there, so a second run is refused, and the password is never printed.
    let again = svetra(&args);
    assert_eq!(again.status.code(), Some(2));
    let err = String::from_utf8_lossy(&again.stderr);
    assert!(err.contains("that database isn't empty"), "{err}");
    assert!(!err.contains("pw@"), "{err}");
}
