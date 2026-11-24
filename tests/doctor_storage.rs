//! `svetra doctor`'s storage buckets, and the one run that is called ready (docs/doctor.md).
//! The runs that start a throwaway database need Docker and are skipped without it.
use serde_json::{Value, json};
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

fn run_in(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "{args:?}");
}

/// A project folder in a fresh temporary directory, with a real `git init`.
struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Project {
        let d = std::env::temp_dir().join(format!(
            "svetra-doctor-storage-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        run_in(&d, &["init", "-q"]);
        Project(d)
    }

    fn write(&self, path: &str, text: &str) -> &Project {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
        self
    }

    fn track_all(&self) {
        run_in(&self.0, &["add", "-f", "."]);
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

const BUCKETS: &str = "create table public.notes (id int primary key);\nalter table public.notes enable row level security;\ninsert into storage.buckets (id, name, public) values ('avatars', 'avatars', true), ('invoices', 'invoices', false);\n";

#[test]
fn a_public_bucket_is_a_warning_and_a_private_one_is_not() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let p = Project::new("buckets");
    p.write("supabase/migrations/20260101000000_buckets.sql", BUCKETS);
    p.track_all();
    let (code, v) = p.json(&["--migrations"]);
    assert_eq!(code, 0, "{v}");
    let buckets: Vec<&Value> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["area"] == "storage")
        .collect();
    assert_eq!(buckets.len(), 1, "{v}");
    assert_eq!(buckets[0]["kind"], "public_bucket");
    assert_eq!(buckets[0]["severity"], "warning");
    assert!(
        buckets[0]["message"].as_str().unwrap().contains("avatars"),
        "{v}"
    );
    assert!(!v.to_string().contains("buckets.invoices"), "{v}");
    assert!(buckets[0].get("file").is_none(), "{v}");
    assert!(
        !v["not_checked"].to_string().contains("Storage buckets"),
        "{v}"
    );
}

#[test]
fn without_a_database_the_buckets_are_not_checked() {
    let p = Project::new("buckets-none");
    let (_, v) = p.json(&[]);
    assert!(
        v["not_checked"].to_string().contains("Storage buckets"),
        "{v}"
    );
}

#[test]
fn a_full_clean_run_is_the_only_one_called_ready() {
    if !docker_up() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let p = Project::new("ready");
    p.write("svetra.yaml", include_str!("fixtures/posts.yaml"));
    p.write("supabase/config.toml", "[auth]\n");
    let compile = svetra(&["compile", p.0.join("svetra.yaml").to_str().unwrap()]);
    assert!(compile.status.success());
    p.write(
        "supabase/migrations/20260101000000_ok.sql",
        "create table public.notes (id int primary key);\nalter table public.notes enable row level security;\n",
    );
    p.track_all();
    let (code, v) = p.json(&["--migrations"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["not_checked"], json!([]), "{v}");
    assert_eq!(v["ready"], true, "{v}");
    let out = svetra(&["doctor", p.0.to_str().unwrap(), "--migrations"]);
    let human = String::from_utf8(out.stdout).unwrap();
    assert!(
        human
            .lines()
            .last()
            .unwrap()
            .contains("The reminders above are yours."),
        "{human}"
    );
}
