//! Scheduled holes (docs/format.md, Scheduled holes): loading, validation and diagnostics.

use svetra::report::validate;

fn doc(hole: &str) -> String {
    format!(
        "svetra: \"0.1\"\nsystem: App\nauth:\n  sign_in: [email]\nroles:\n  member: {{}}\nentities:\n  Invite:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user, required: true }}\npolicies:\n  own:\n    on: Invite\n    allow: [select]\n    when: \"owner == auth.user\"\nholes:\n  expire_invites:\n{hole}"
    )
}

fn found(hole: &str) -> Vec<(String, usize, String)> {
    validate(&doc(hole))
        .into_iter()
        .filter(|d| !matches!(d.code.id(), "SVT009" | "SVT010"))
        .map(|d| (d.code.id().to_string(), d.line, d.hint))
        .collect()
}

const GOOD: &str =
    "    kind: schedule\n    schedule: \"0 3 * * *\"\n    reads: [Invite]\n    writes: [Invite]\n";

#[test]
fn a_valid_schedule_has_no_diagnostics() {
    assert_eq!(found(GOOD), vec![]);
}

#[test]
fn svt037_a_bad_cron_says_which_field() {
    for (bad, why) in [
        ("0 25 * * *", "field 2 (hour)"),
        ("0 3 * *", "has 4 fields"),
        ("0 3 * * mon", "is a name"),
    ] {
        let src = doc(&GOOD.replace("0 3 * * *", bad));
        let d = validate(&src)
            .into_iter()
            .find(|d| d.code.id() == "SVT037")
            .unwrap_or_else(|| panic!("no SVT037 for {bad}"));
        assert!(d.message.contains(why), "{}", d.message);
        assert_eq!(d.line, 20, "points at the schedule value");
    }
}

#[test]
fn svt020_keys_a_schedule_cannot_take_say_why() {
    for key in [
        "input: { n: int }",
        "output: int",
        "as: anyone",
        "when: \"true\"",
    ] {
        let src = doc(&format!("{GOOD}    {key}\n"));
        let d = validate(&src)
            .into_iter()
            .find(|d| d.code.id() == "SVT020")
            .unwrap_or_else(|| panic!("no SVT020 for {key}"));
        assert!(d.message.contains("isn't allowed on a `schedule` hole"));
        assert!(d.hint.contains("kind, schedule, reads, writes, grants"));
    }
}

#[test]
fn svt020_a_function_cannot_take_a_schedule() {
    let src = doc("    kind: function\n    schedule: \"0 3 * * *\"\n");
    assert!(validate(&src).iter().any(|d| d.code.id() == "SVT020"));
}

#[test]
fn svt028_a_schedule_needs_a_schedule() {
    let src = doc("    kind: schedule\n    reads: [Invite]\n");
    assert!(validate(&src).iter().any(|d| d.code.id() == "SVT028"));
}

#[test]
fn svt023_an_unknown_kind_lists_the_kinds() {
    let src = doc("    kind: queue\n");
    let d = validate(&src)
        .into_iter()
        .find(|d| d.code.id() == "SVT023")
        .expect("SVT023");
    assert!(d.hint.contains("function, schedule, trigger, webhook"));
}

#[test]
fn svt040_grants_on_a_schedule_warn() {
    let src = doc(&format!("{GOOD}    grants: [member]\n"));
    let d = validate(&src)
        .into_iter()
        .find(|d| d.code.id() == "SVT040")
        .expect("SVT040");
    assert_eq!(d.severity(), svetra::diag::Severity::Warning);
}
