use serde_json::{Value, json};
use std::process::Command;
use svetra::inspect::{Severity, findings, query, redact};

fn leaky() -> Value {
    serde_json::from_str(include_str!("fixtures/check/leaky_app.json")).unwrap()
}

fn svetra(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(args)
        .output()
        .unwrap()
}

/// The snapshot was taken from tests/pg/check/leaky_app.sql applied to PostgreSQL 16 with the
/// Supabase stand-in; scripts/pg-check.sh takes it again and compares with the same golden file.
#[test]
fn every_planted_leak_is_found_and_matches_the_golden_file() {
    let out = svetra(&[
        "check",
        "--snapshot",
        "tests/fixtures/check/leaky_app.json",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(1), "critical findings exit 1");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        include_str!("golden/leaky_app.check.json")
    );
    let found: Vec<(&str, String)> = findings(&leaky())
        .into_iter()
        .map(|f| (f.kind, f.object))
        .collect();
    for (kind, object) in [
        ("rls_off", "public.orders"),
        ("open_write", "public.comments"),
        ("view_bypass", "public.user_emails"),
        ("view_bypass", "public.order_totals"),
        ("open_read", "public.posts"),
        ("signed_in_write", "public.notes"),
        ("user_metadata", "public.notes"),
        ("definer_function", "public.make_admin(target uuid)"),
        ("mutable_search_path", "public.make_admin(target uuid)"),
        ("no_policy", "public.audit"),
    ] {
        assert!(
            found.iter().any(|(k, o)| *k == kind && o == object),
            "{kind} on {object}: {found:?}"
        );
    }
    assert!(
        found.iter().all(|(_, o)| !o.contains("private_stuff")),
        "unreachable tables aren't reported"
    );
}

#[test]
fn severities_follow_the_spec() {
    let all = findings(&leaky());
    let sev = |kind: &str, object: &str| {
        all.iter()
            .find(|f| f.kind == kind && f.object == object)
            .unwrap()
            .severity
    };
    assert_eq!(sev("rls_off", "public.orders"), Severity::Critical);
    assert_eq!(sev("view_bypass", "public.user_emails"), Severity::Critical);
    assert_eq!(sev("view_bypass", "public.order_totals"), Severity::Warning);
    assert_eq!(
        sev("definer_function", "public.make_admin(target uuid)"),
        Severity::Warning
    );
    // Most severe first.
    assert!(all.windows(2).all(|w| w[0].severity <= w[1].severity));
}

#[test]
fn a_function_only_signed_in_people_can_call_is_a_note() {
    let snap = json!({ "tables": [], "policies": [], "views": [], "functions": [
        { "schema": "public", "name": "transfer", "args": "", "security_definer": true, "search_path_set": true,
          "anon_execute": false, "authenticated_execute": true }
    ]});
    let f = findings(&snap);
    assert_eq!(f.len(), 1);
    assert_eq!(
        (f[0].kind, f[0].severity),
        ("definer_function", Severity::Info)
    );
}

#[test]
fn roles_as_a_postgres_array_literal_count_too() {
    let snap = json!({ "tables": [], "views": [], "functions": [], "policies": [
        { "schema": "public", "table": "t", "name": "p", "permissive": "PERMISSIVE", "roles": "{public}",
          "cmd": "SELECT", "using": "true", "check": null }
    ]});
    assert_eq!(findings(&snap)[0].kind, "open_read");
}

#[test]
fn the_query_only_reads() {
    let q = query(&["public".into(), "api".into()]);
    assert!(q.starts_with("select json_build_object("));
    assert!(!q.contains(';'), "one statement");
    for word in [
        "insert into",
        "update ",
        "delete from",
        "create ",
        "drop ",
        "alter ",
        "grant ",
        "truncate",
    ] {
        assert!(!q.to_lowercase().contains(word), "{word}");
    }
    assert!(q.contains("in ('public', 'api')"));
}

#[test]
fn passwords_are_never_printed() {
    assert_eq!(
        redact("postgresql://postgres:s3cret@db.x.supabase.co:5432/postgres"),
        "postgresql://postgres:***@db.x.supabase.co:5432/postgres"
    );
    assert_eq!(
        redact("postgresql://localhost/db"),
        "postgresql://localhost/db"
    );
}

#[test]
fn usage_errors_exit_2() {
    assert_eq!(svetra(&["check"]).status.code(), Some(2));
    let bad = std::env::temp_dir().join(format!("svetra-bad-snapshot-{}.json", std::process::id()));
    std::fs::write(&bad, "not json").unwrap();
    assert_eq!(
        svetra(&["check", "--snapshot", bad.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    let q = svetra(&["check", "--print-query"]);
    assert_eq!(q.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&q.stdout).starts_with("select json_build_object("));
}
