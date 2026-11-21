//! `svetra infer --check-draft` (docs/infer.md, Differential testing). The runs that start a
//! throwaway database need Docker and Supabase's database image, and are skipped without them.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn svetra(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(args)
        .output()
        .unwrap()
}

fn ready() -> bool {
    svetra::migrations::docker_available()
        && Command::new("docker")
            .args(["image", "inspect", svetra::migrations::SUPABASE_IMAGE])
            .output()
            .is_ok_and(|o| o.status.success())
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("svetra-verify-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const SHOP: &str = include_str!("pg/infer/shop_app.sql");
const STAFF_READ: &str = "create policy orders_staff_read on public.orders for select to authenticated using ((auth.jwt() ->> 'role') = 'staff');\n";

/// The shop's draft as `infer` wrote it.
fn raw_draft() -> String {
    let out = svetra(&[
        "infer",
        "--snapshot",
        "tests/fixtures/infer/shop_app.json",
        "--out",
        "-",
    ]);
    String::from_utf8(out.stdout).unwrap()
}

/// The shop's draft with Orders ported: its one untranslated policy and the note about it
/// removed, as a person would after writing the policy by hand.
fn draft() -> String {
    raw_draft()
        .lines()
        .filter(|l| {
            !l.contains("# UNTRANSLATED policy orders_staff_read")
                && !l.contains("# policies: 5 of 6")
        })
        .map(|l| format!("{l}\n"))
        .collect()
}

/// The shop's SQL as a migrations folder, edited by `edit`, and the draft in a file.
fn setup(name: &str, edit: impl Fn(&str) -> String) -> (PathBuf, PathBuf) {
    let dir = temp(name);
    let migrations = dir.join("migrations");
    std::fs::create_dir_all(&migrations).unwrap();
    std::fs::write(migrations.join("20260101000000_shop.sql"), edit(SHOP)).unwrap();
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, draft()).unwrap();
    (migrations, file)
}

fn run(file: &Path, migrations: &Path) -> (Option<i32>, String, String) {
    let out = svetra(&[
        "infer",
        "--check-draft",
        file.to_str().unwrap(),
        "--migrations",
        migrations.to_str().unwrap(),
    ]);
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// The summary line for one entity: its words after the name.
fn line<'a>(text: &'a str, entity: &str) -> &'a str {
    text.lines()
        .find(|l| l.trim_start().starts_with(entity) && l.contains("  "))
        .unwrap_or_else(|| panic!("no line for {entity} in:\n{text}"))
}

#[test]
fn every_translated_table_passes_against_the_database_it_came_from() {
    if !ready() {
        eprintln!("skipped: Docker or Supabase's database image isn't there");
        return;
    }
    let (migrations, file) = setup("agree", |sql| sql.replace(STAFF_READ, ""));
    let (code, text, err) = run(&file, &migrations);
    assert_eq!(code, Some(0), "{text}{err}");
    assert!(line(&text, "Products").contains("agrees"), "{text}");
    assert!(line(&text, "Orders").contains("agrees"), "{text}");
    // Row-level security is off on order_items, and profiles' own-row policy didn't translate.
    assert!(line(&text, "OrderItems").contains("skipped"), "{text}");
    assert!(line(&text, "Profiles").contains("skipped"), "{text}");
    assert!(text.contains("does what the draft says"), "{text}");
}

#[test]
fn a_table_with_an_untranslated_policy_is_skipped_not_failed() {
    if !ready() {
        eprintln!("skipped: Docker or Supabase's database image isn't there");
        return;
    }
    // The draft as `infer` wrote it, against the database with the staff policy it left out.
    let (migrations, file) = setup("narrower", str::to_string);
    std::fs::write(&file, raw_draft()).unwrap();
    let (code, text, err) = run(&file, &migrations);
    assert_eq!(code, Some(0), "{text}{err}");
    let orders = line(&text, "Orders");
    assert!(
        orders.contains("skipped") && orders.contains("narrower"),
        "{text}"
    );
}

#[test]
fn a_policy_that_differs_fails_the_checks_that_name_it() {
    if !ready() {
        eprintln!("skipped: Docker or Supabase's database image isn't there");
        return;
    }
    // The database lets every signed-in user see every order, and keeps anonymous people from
    // products; the draft says the opposite on both.
    let (migrations, file) = setup("differ", |sql| {
        sql.replace(STAFF_READ, "")
            .replace(
                "for select to authenticated using (customer = auth.uid());",
                "for select to authenticated using (true);",
            )
            .replace(
                "on public.products for select using (true)",
                "on public.products for select to authenticated using (true)",
            )
    });
    let (code, text, err) = run(&file, &migrations);
    assert_eq!(code, Some(1), "{text}{err}");
    assert!(line(&text, "Orders").contains("DISAGREES"), "{text}");
    assert!(line(&text, "Products").contains("DISAGREES"), "{text}");
    for check in [
        "bob can't see Orders a (the draft says denied, the database says allowed)",
        "carol can't see Orders b (the draft says denied, the database says allowed)",
        "anon can see Products a (the draft says allowed, the database says denied)",
    ] {
        assert!(text.contains(check), "missing {check:?} in:\n{text}");
    }
    // What still matches isn't reported: alice does see her own order.
    assert!(!text.contains("alice can see Orders a"), "{text}");
    assert!(text.contains("disagree."), "{text}");
}

#[test]
fn a_draft_that_does_not_validate_stops_before_any_database() {
    let dir = temp("invalid");
    let file = dir.join("svetra.yaml");
    std::fs::write(
        &file,
        "svetra: \"0.1\"\nsystem: App\nentities:\n  A:\n    fields:\n      id: { type: nope }\n",
    )
    .unwrap();
    let out = svetra(&[
        "infer",
        "--check-draft",
        file.to_str().unwrap(),
        "--migrations",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("doesn't validate"), "{err}");
}

#[test]
fn it_needs_exactly_one_source_and_a_file() {
    for args in [
        vec!["infer", "--check-draft", "svetra.yaml"],
        vec![
            "infer",
            "--check-draft",
            "svetra.yaml",
            "--migrations",
            "--local",
        ],
        vec!["infer", "--check-draft"],
        vec![
            "infer",
            "--check-draft",
            "svetra.yaml",
            "--migrations",
            "--out",
            "x",
        ],
    ] {
        let out = svetra(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}
