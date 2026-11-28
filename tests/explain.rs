use svetra::load::load;
use svetra::understand::explain::explain;

/// Fields named for an action read as that action, either way round and negated.
#[test]
fn action_fields_read_as_actions() {
    let src = "svetra: \"0.1\"\nsystem: Chores\nentities:\n  Chore:\n    fields:\n      id: { type: uuid, primary: true }\n      created_by: { type: user, required: true }\n      assigned_to: { type: user }\n      owner: { type: user, required: true }\npolicies:\n  creators: { on: Chore, allow: [select, insert], when: \"created_by == auth.user\" }\n  assignees: { on: Chore, allow: [select], when: \"auth.user == assigned_to\" }\n  others: { on: Chore, allow: [update], when: \"owner == auth.user and assigned_to != auth.user\" }\n";
    let (doc, diags) = load(src);
    assert!(diags.is_empty(), "{diags:#?}");
    let out = explain(&doc, &[]);
    for line in [
        "Signed-in people can see and create a chore when it was created by them.",
        "Signed-in people can see a chore when it is assigned to them.",
        "Signed-in people can change a chore when its owner is them and it is not assigned to them.",
    ] {
        assert!(out.contains(line), "missing {line:?} in:\n{out}");
    }
}

/// An audited entity and an audited hole say so; an unaudited one says nothing about audit.
#[test]
fn audited_entities_and_holes_say_they_are_recorded() {
    let (doc, diags) = load(include_str!("fixtures/audited.yaml"));
    assert!(
        diags.is_empty()
            || diags
                .iter()
                .all(|d| d.severity() != svetra::diag::Severity::Error)
    );
    let out = explain(&doc, &diags);
    assert!(out.contains(
        "Every change to notes is recorded in the audit log: who made it, when, and the row before and after."
    ));
    assert_eq!(
        out.matches(
            "every call is recorded in the audit log, with who made it and what they sent."
        )
        .count(),
        2
    );
    let (doc, diags) = load(include_str!("fixtures/finledger.yaml"));
    assert!(!explain(&doc, &diags).contains("audit log"));
}

#[test]
fn context_marks_audited_entities_and_holes() {
    let (doc, _) = load(include_str!("fixtures/audited.yaml"));
    let out = svetra::understand::context::context(&doc);
    assert!(out.contains("\naudit Note: "), "{out}");
    assert!(
        out.contains("; when note.owner == auth.user; audit\n"),
        "{out}"
    );
    assert!(
        out.contains("hole sweep: () -> none; writes Note; audit\n"),
        "{out}"
    );
}

/// A hole with `require` says what a call must send, in words; a hole without says nothing.
#[test]
fn holes_with_require_say_what_input_is_refused() {
    let (doc, diags) = load(include_str!("fixtures/validated.yaml"));
    let out = explain(&doc, &diags);
    assert!(out.contains(
        "a call with bad input is refused unless the amount is more than 0, and the amount is at most 10000."
    ));
    let (doc, diags) = load(include_str!("fixtures/finledger.yaml"));
    assert!(!explain(&doc, &diags).contains("bad input"));
}

#[test]
fn context_lists_a_holes_requirements() {
    let (doc, _) = load(include_str!("fixtures/validated.yaml"));
    let out = svetra::understand::context::context(&doc);
    assert!(
        out.contains("; when wallet.owner == auth.user; require amount > 0 | amount <= 10000\n"),
        "{out}"
    );
}

#[test]
fn a_hole_with_a_retry_key_says_what_a_repeat_does() {
    let (doc, diags) = load(include_str!("fixtures/retries.yaml"));
    let out = explain(&doc, &diags);
    assert!(out.contains("a call repeated with the same request by the same person returns the first answer and runs nothing"));
    assert!(out.contains("with the same tag"));
    let (doc, diags) = load(include_str!("fixtures/finledger.yaml"));
    assert!(!explain(&doc, &diags).contains("repeated"));
    let (doc, _) = load(include_str!("fixtures/retries.yaml"));
    let ctx = svetra::understand::context::context(&doc);
    assert!(ctx.contains("; idempotent request; audit\n"), "{ctx}");
    assert!(ctx.contains("; idempotent tag\n"), "{ctx}");
}
