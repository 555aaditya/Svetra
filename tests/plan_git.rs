//! `plan --from-git`, `--no-write` and `--markdown` (docs/action.md), on a throwaway git repository.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BASE: &str = r#"svetra: "0.1"
system: Shop
entities:
  Booking:
    fields:
      id:       { type: uuid, primary: true }
      customer: { type: user, required: true }
      notes:    { type: text }
  Comment:
    fields:
      id:   { type: uuid, primary: true }
      body: { type: text }
policies:
  own_bookings: { on: Booking, allow: [select], when: "customer == auth.user" }
  all_comments: { on: Comment, allow: [select], when: "true" }
"#;

/// A rename, a removed entity and a rule that now lets everyone in.
const CHANGED: &str = r#"svetra: "0.1"
system: Shop
entities:
  Booking:
    fields:
      id:     { type: uuid, primary: true }
      client: { type: user, required: true, was: customer }
      notes:  { type: text }
policies:
  all_bookings: { on: Booking, allow: [select], when: "true" }
"#;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A repository whose `base` branch has BASE and whose working copy has CHANGED.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("svetra-plangit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "base"]);
    std::fs::write(dir.join("svetra.yaml"), BASE).unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    git(&dir, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(dir.join("svetra.yaml"), CHANGED).unwrap();
    dir
}

fn svetra(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn from_git_gives_the_same_plan_as_from_with_that_file() {
    let dir = repo("same");
    std::fs::write(dir.join("old.yaml"), BASE).unwrap();
    let a = svetra(
        &dir,
        &["plan", "--from-git", "base", "--no-write", "--json"],
    );
    let b = svetra(
        &dir,
        &["plan", "--from", "old.yaml", "--no-write", "--json"],
    );
    let (mut a, mut b): (serde_json::Value, serde_json::Value) = (
        serde_json::from_str(&text(&a)).unwrap(),
        serde_json::from_str(&text(&b)).unwrap(),
    );
    assert_eq!(a["from"], "base:svetra.yaml");
    a["from"] = serde_json::Value::Null;
    b["from"] = serde_json::Value::Null;
    assert_eq!(a, b);
}

#[test]
fn a_missing_ref_and_a_missing_file_each_say_which() {
    let dir = repo("missing");
    let o = svetra(&dir, &["plan", "--from-git", "nope"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        err(&o).contains("git ref nope doesn't exist"),
        "{}",
        err(&o)
    );
    // A branch where the file isn't there yet.
    git(&dir, &["checkout", "-q", "--orphan", "empty"]);
    git(&dir, &["rm", "-rqf", "--cached", "."]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "empty"]);
    let o = svetra(&dir, &["plan", "--from-git", "empty"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        err(&o).contains("svetra.yaml isn't in git empty"),
        "{}",
        err(&o)
    );
}

#[test]
fn from_and_from_git_or_json_and_markdown_are_usage_errors() {
    let dir = repo("usage");
    for args in [
        ["plan", "--from", "x.yaml", "--from-git", "base"],
        ["plan", "--json", "--markdown", "--from-git", "base"],
    ] {
        let o = svetra(&dir, &args);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", err(&o));
    }
}

#[test]
fn no_write_writes_nothing_and_says_what_it_would_have() {
    let dir = repo("nowrite");
    // Everything waiting is accepted, so a plan that isn't blocked would write the rename.
    let accept = "remove_entity,wider_access";
    let o = svetra(
        &dir,
        &[
            "plan",
            "--from-git",
            "base",
            "--accept",
            accept,
            "--no-write",
        ],
    );
    assert_eq!(o.status.code(), Some(0), "{}{}", text(&o), err(&o));
    assert!(text(&o).contains("would write"), "{}", text(&o));
    assert!(!dir.join("supabase").exists(), "wrote into the checkout");
    // Without the flag the same plan writes it.
    let o = svetra(&dir, &["plan", "--from-git", "base", "--accept", accept]);
    assert_eq!(o.status.code(), Some(0));
    assert!(dir.join("supabase/migrations").is_dir());
}

#[test]
fn markdown_matches_the_golden_comment_and_exits_one_while_waiting() {
    let dir = repo("markdown");
    let o = svetra(
        &dir,
        &["plan", "--from-git", "base", "--markdown", "--no-write"],
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(text(&o), include_str!("golden/plan.md"));
}

#[test]
fn markdown_says_what_was_accepted_and_when_nothing_changed() {
    let dir = repo("accepted");
    let o = svetra(
        &dir,
        &[
            "plan",
            "--from-git",
            "base",
            "--markdown",
            "--no-write",
            "--accept",
            "remove_entity,wider_access",
        ],
    );
    assert_eq!(o.status.code(), Some(0));
    let t = text(&o);
    assert!(
        t.contains("**Accepted:** `remove_entity`, `wider_access`."),
        "{t}"
    );
    assert!(!t.contains("Needs a yes"));
    git(&dir, &["checkout", "-q", "svetra.yaml"]);
    let o = svetra(&dir, &["plan", "--from-git", "feature", "--markdown"]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        text(&o),
        "### svetra plan: no changes to Shop (from feature:svetra.yaml)\n"
    );
}
