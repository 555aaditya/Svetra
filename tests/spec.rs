//! docs/format.md is the authority. These tests fail when the code and the spec disagree.

use std::collections::BTreeSet;
use svetra::diag::Code;

const ALL: &[Code] = &[
    Code::Svt001,
    Code::Svt002,
    Code::Svt003,
    Code::Svt004,
    Code::Svt005,
    Code::Svt006,
    Code::Svt007,
    Code::Svt008,
    Code::Svt009,
    Code::Svt010,
    Code::Svt019,
    Code::Svt020,
    Code::Svt021,
    Code::Svt022,
    Code::Svt023,
    Code::Svt024,
    Code::Svt025,
    Code::Svt026,
    Code::Svt027,
    Code::Svt028,
    Code::Svt029,
];

/// Rows of the spec's Diagnostics table: (code, severity, action).
fn spec_rows() -> Vec<(String, String, String)> {
    let spec = include_str!("../docs/format.md");
    let table = spec
        .split("## Diagnostics")
        .nth(1)
        .expect("Diagnostics section")
        .split("\n## ")
        .next()
        .unwrap();
    table
        .lines()
        .filter(|l| l.starts_with("| `SVT"))
        .map(|l| {
            let cells: Vec<&str> = l.split('|').map(|c| c.trim().trim_matches('`')).collect();
            (
                cells[1].to_string(),
                cells[2].to_string(),
                cells[4].to_string(),
            )
        })
        .collect()
}

#[test]
fn every_code_is_in_the_spec_and_every_spec_code_exists() {
    let in_code: BTreeSet<String> = ALL.iter().map(|c| c.id().to_string()).collect();
    let in_spec: BTreeSet<String> = spec_rows().into_iter().map(|r| r.0).collect();
    assert_eq!(in_code, in_spec, "codes in src/diag.rs vs docs/format.md");
}

#[test]
fn severity_and_action_match_the_spec() {
    for (id, severity, action) in spec_rows() {
        let code = ALL.iter().find(|c| c.id() == id).unwrap();
        let sev = format!("{:?}", code.severity()).to_lowercase();
        assert_eq!(sev, severity, "{id} severity");
        assert_eq!(code.action(), action, "{id} action");
    }
}

/// Fails to compile when a `Code` variant is added without listing it in `ALL`.
#[allow(dead_code)]
fn all_is_exhaustive(c: Code) {
    match c {
        Code::Svt001
        | Code::Svt002
        | Code::Svt003
        | Code::Svt004
        | Code::Svt005
        | Code::Svt006
        | Code::Svt007
        | Code::Svt008
        | Code::Svt009
        | Code::Svt010
        | Code::Svt019
        | Code::Svt020
        | Code::Svt021
        | Code::Svt022
        | Code::Svt023
        | Code::Svt024
        | Code::Svt025
        | Code::Svt026
        | Code::Svt027
        | Code::Svt028
        | Code::Svt029 => {}
    }
}
