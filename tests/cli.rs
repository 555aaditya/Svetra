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
