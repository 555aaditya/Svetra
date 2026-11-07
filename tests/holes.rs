use svetra::load::load;
use svetra::report::validate;

/// The ledger with `when` and `as` on its hole; `extra` replaces the hole's last two lines.
fn ledger(hole_tail: &str) -> String {
    include_str!("fixtures/finledger.yaml").replace(
        "    reads: [Account]\n    writes: [Account, Transaction]",
        &format!("    reads: [Account]\n    writes: [Account, Transaction]\n{hole_tail}"),
    )
}

fn codes(src: &str) -> Vec<(String, String)> {
    validate(src)
        .into_iter()
        .filter(|d| d.code.id() != "SVT009")
        .map(|d| (d.code.id().to_string(), d.message))
        .collect()
}

#[test]
fn when_and_as_load_and_check() {
    let src =
        ledger("    as: authenticated\n    when: \"source.owner == auth.user and amount > 0\"");
    assert!(codes(&src).is_empty(), "{:?}", codes(&src));
    let (doc, _) = load(&src);
    assert_eq!(
        doc.holes[0].when.as_deref(),
        Some("source.owner == auth.user and amount > 0")
    );
}

#[test]
fn when_is_checked_against_the_inputs() {
    for (when, needle) in [
        ("balance > 0", "has no field `balance`"),
        ("source.nope == auth.user", "has no field `nope`"),
        ("amount.owner == auth.user", "not a reference"),
        ("amount", "must be true or false"),
    ] {
        let found = codes(&ledger(&format!("    when: \"{when}\"")));
        assert_eq!(found.len(), 1, "{when}: {found:?}");
        assert_eq!(found[0].0, "SVT003", "{when}");
        assert!(found[0].1.contains(needle), "{when}: {:?}", found[0].1);
    }
}

#[test]
fn has_role_works_in_hole_guards() {
    let src = ledger("    when: \"has_role('auditor')\"").replace(
        "entities:",
        "roles:\n  auditor: { description: \"Checks the books\" }\n\nentities:",
    );
    assert!(codes(&src).is_empty(), "{:?}", codes(&src));
    let found = codes(&ledger("    when: \"has_role('ghost')\""));
    assert_eq!(found[0].0, "SVT006");
}

#[test]
fn as_must_be_a_known_role() {
    let found = codes(&ledger("    as: everyone"));
    assert_eq!(found[0].0, "SVT029");
}
