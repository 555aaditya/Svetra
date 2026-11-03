use std::process::{Command, Output};

fn svetra(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(args)
        .output()
        .expect("run svetra")
}

#[test]
fn version_prints_crate_version() {
    let out = svetra(&["--version"]);
    assert!(out.status.success());
    let expected = format!("svetra {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
}

#[test]
fn unknown_argument_is_a_usage_error() {
    let out = svetra(&["bogus"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown argument 'bogus'"));
}

#[test]
fn validate_accepts_the_spec_example() {
    let out = svetra(&["validate", "tests/fixtures/finledger.yaml"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("is valid (0 warnings)"));
}

#[test]
fn validate_json_reports_errors_and_exits_1() {
    let dir = std::env::temp_dir().join(format!("svetra-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, "svetra: \"0.1\"\nsystem: App\nentities:\n  A:\n    fields:\n      id: { type: uuid, primary: true }\n").unwrap();
    let out = svetra(&["validate", file.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"valid\":false"));
    assert!(stdout.contains("\"code\":\"SVT001\""));
    assert!(stdout.contains("\"column\":3"));
}

#[test]
fn validate_missing_file_is_a_usage_error() {
    let out = svetra(&["validate", "does-not-exist.yaml"]);
    assert_eq!(out.status.code(), Some(2));
}
