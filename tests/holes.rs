use svetra::load::load;
use svetra::report::validate;

/// The ledger with `when` and `as` on its hole; `extra` replaces the hole's last two lines.
fn ledger(hole_tail: &str) -> String {
    include_str!("fixtures/finledger.yaml")
        .replace("    when: \"source.owner == auth.user\"\n", "")
        .replace(
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

#[test]
fn when_can_only_read_through_declared_entities() {
    let src = ledger("    when: \"source.owner == auth.user\"").replace(
        "    reads: [Account]\n    writes: [Account, Transaction]",
        "    writes: [Transaction]",
    );
    let found: Vec<_> = codes(&src)
        .into_iter()
        .filter(|(c, _)| c == "SVT003")
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].1.contains("reads Account through `source`"),
        "{found:?}"
    );
}

#[test]
fn access_tests_check_each_holes_grants_and_guard() {
    let (doc, _) = load(include_str!("fixtures/finledger.yaml"));
    let t = svetra::compile::access_tests::tests(&doc);
    for (want, name) in [
        (
            "select, insert, update, delete",
            "transfer can change Account",
        ),
        ("refused", "anon can't call transfer on the a rows"),
        ("let through", "alice can call transfer on the a rows"),
        ("refused", "alice can't call transfer on the b rows"),
        ("let through", "bob can call transfer on the b rows"),
    ] {
        let line = t
            .lines()
            .find(|l| l.ends_with(&format!("'{}');", name.replace('\'', "''"))));
        let line = line.unwrap_or_else(|| panic!("no check named {name}"));
        assert!(line.contains(&format!("'{want}'")), "{name}: {line}");
    }
    // A hole with no `when` lets every signed-in person through, and checks its grants.
    let (posts, _) = load(include_str!("fixtures/posts.yaml"));
    let t = svetra::compile::access_tests::tests(&posts);
    assert!(t.contains("'bob can call write_audit on the a rows');"));
    assert!(t.contains("'none', 'write_audit can''t touch Post');"));
}
