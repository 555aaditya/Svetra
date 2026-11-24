//! Triggered holes (docs/format.md, Triggered holes): loading, validation and diagnostics.

use svetra::report::validate;

/// An app with two entities and the holes given, each as the YAML under `holes:`.
fn doc(holes: &str) -> String {
    format!(
        "svetra: \"0.1\"\nsystem: App\nauth:\n  sign_in: [email]\nroles:\n  member: {{}}\nentities:\n  Order:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user, required: true }}\n      status: {{ type: enum, values: [open, paid], default: open }}\n  Note:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user, required: true }}\npolicies:\n  own:\n    on: Order\n    allow: [select, insert, update]\n    when: \"owner == auth.user\"\n  own_notes:\n    on: Note\n    allow: [select]\n    when: \"owner == auth.user\"\nholes:\n{holes}"
    )
}

fn found(holes: &str) -> Vec<(String, String)> {
    validate(&doc(holes))
        .into_iter()
        .filter(|d| !matches!(d.code.id(), "SVT009" | "SVT010"))
        .map(|d| (d.code.id().to_string(), d.message))
        .collect()
}

fn codes(holes: &str) -> Vec<String> {
    found(holes).into_iter().map(|(c, _)| c).collect()
}

const GOOD: &str = "  notify:\n    kind: trigger\n    on: Order\n    events: [update]\n    when: \"status == 'paid' and old.status != 'paid'\"\n    reads: [Order]\n    writes: [Note]\n";

#[test]
fn a_valid_trigger_has_no_diagnostics() {
    assert_eq!(found(GOOD), vec![]);
}

#[test]
fn svt028_a_trigger_needs_on_and_events() {
    for missing in ["    on: Order\n", "    events: [update]\n"] {
        let hole = GOOD.replace(missing, "");
        assert_eq!(codes(&hole), ["SVT028"], "without {missing:?}");
    }
}

#[test]
fn svt029_events_are_insert_update_or_delete() {
    for bad in ["[select]", "[]", "[truncate, update]"] {
        let hole = GOOD.replace("[update]", bad);
        assert!(codes(&hole).contains(&"SVT029".to_string()), "{bad}");
    }
}

#[test]
fn svt002_on_names_an_entity() {
    let hole = GOOD.replace("on: Order", "on: Basket");
    assert!(codes(&hole).contains(&"SVT002".to_string()));
}

#[test]
fn svt020_keys_a_trigger_cannot_take_say_why() {
    for key in [
        "input: { n: int }",
        "output: int",
        "as: anyone",
        "audit: true",
        "schedule: \"0 3 * * *\"",
    ] {
        let src = doc(&format!("{GOOD}    {key}\n"));
        let d = validate(&src)
            .into_iter()
            .find(|d| d.code.id() == "SVT020")
            .unwrap_or_else(|| panic!("no SVT020 for {key}"));
        assert!(d.message.contains("isn't allowed on a `trigger` hole"));
        assert!(
            d.hint
                .contains("kind, on, events, when, reads, writes, grants")
        );
    }
}

#[test]
fn svt020_a_function_cannot_take_on_or_events() {
    let src = doc("  f:\n    kind: function\n    on: Order\n");
    assert!(validate(&src).iter().any(|d| d.code.id() == "SVT020"));
}

#[test]
fn svt003_old_is_for_an_update_only() {
    for events in ["[insert]", "[delete]", "[insert, update]"] {
        let hole = GOOD.replace("[update]", events);
        let d = found(&hole)
            .into_iter()
            .find(|(c, _)| c == "SVT003")
            .unwrap_or_else(|| panic!("no SVT003 for {events}"));
        assert!(d.1.contains("`old.status` can only be used when `events` is exactly [update]"));
    }
}

#[test]
fn svt003_a_trigger_condition_reads_only_the_row() {
    for (when, why) in [
        ("owner == auth.user", "`auth.user`"),
        ("has_role('member')", "`has_role`"),
        ("status", "evaluates to text"),
        ("old.missing == 1", "no field `missing`"),
        ("status == 'gone'", "no value 'gone'"),
    ] {
        let hole = GOOD.replace("status == 'paid' and old.status != 'paid'", when);
        let d = found(&hole)
            .into_iter()
            .find(|(c, _)| c == "SVT003" || c == "SVT006")
            .unwrap_or_else(|| panic!("no diagnostic for {when}"));
        assert!(d.1.contains(why), "{when}: {}", d.1);
    }
}

#[test]
fn svt003_a_trigger_condition_cannot_read_another_entity() {
    let src = doc(&GOOD.replace("on: Order", "on: Note").replace(
        "status == 'paid' and old.status != 'paid'",
        "owner.status == 'paid'",
    ));
    assert!(validate(&src).iter().any(|d| d.code.id() == "SVT003"));
}

#[test]
fn svt038_a_trigger_that_writes_its_own_entity_loops() {
    let hole = GOOD.replace("writes: [Note]", "writes: [Order, Note]");
    let (code, message) = found(&hole).remove(0);
    assert_eq!(code, "SVT038");
    assert!(
        message.contains("its own change fires it again"),
        "{message}"
    );
}

#[test]
fn svt038_two_triggers_that_write_each_others_entity_loop() {
    let two = format!(
        "{GOOD}  echo:\n    kind: trigger\n    on: Note\n    events: [insert]\n    writes: [Order]\n"
    );
    let all = found(&two);
    assert_eq!(all.len(), 1, "one report per cycle: {all:?}");
    assert_eq!(all[0].0, "SVT038");
    assert!(
        all[0].1.contains("notify -> echo -> notify"),
        "{}",
        all[0].1
    );
}

#[test]
fn svt038_names_the_shortest_cycle_through_three_holes() {
    let three = "  a:\n    kind: trigger\n    on: Order\n    events: [insert]\n    writes: [Note]\n  b:\n    kind: trigger\n    on: Note\n    events: [insert]\n    writes: [Mid]\n  c:\n    kind: trigger\n    on: Mid\n    events: [insert]\n    writes: [Order]\n";
    let src = doc(three).replace(
        "policies:",
        "  Mid:\n    fields:\n      id: { type: uuid, primary: true }\n      owner: { type: user, required: true }\npolicies:\n  own_mid:\n    on: Mid\n    allow: [select]\n    when: \"owner == auth.user\"",
    );
    let d: Vec<_> = validate(&src)
        .into_iter()
        .filter(|d| d.code.id() == "SVT038")
        .collect();
    assert_eq!(d.len(), 1);
    assert!(
        d[0].message.contains("a -> b -> c -> a"),
        "{}",
        d[0].message
    );
}

#[test]
fn a_chain_of_triggers_without_a_cycle_is_fine() {
    // notify writes Note, and echo fires on Note but writes Order's sibling: no way back.
    let chain = format!("{GOOD}  echo:\n    kind: trigger\n    on: Note\n    events: [insert]\n");
    assert_eq!(found(&chain), vec![]);
}

#[test]
fn a_function_hole_that_writes_a_triggered_entity_is_not_a_loop() {
    let hole =
        format!("{GOOD}  fix:\n    kind: function\n    writes: [Order]\n    reads: [Note]\n");
    assert_eq!(found(&hole), vec![]);
}

#[test]
fn svt040_grants_on_a_trigger_warn() {
    let src = doc(&format!("{GOOD}    grants: [member]\n"));
    let d = validate(&src)
        .into_iter()
        .find(|d| d.code.id() == "SVT040")
        .expect("SVT040");
    assert_eq!(d.severity(), svetra::diag::Severity::Warning);
    assert!(d.message.contains("runs when a row changes"));
}

/// `svetra diff` and `plan` (docs/change.md): the kinds of change a triggered hole causes.
mod change {
    use svetra::change::{Data, plan};
    use svetra::load::load;

    const FILE: &str = "svetra: \"0.1\"\nsystem: App\nentities:\n  Order:\n    fields:\n      id: { type: uuid, primary: true }\n      status: { type: enum, values: [open, paid], default: open }\n  Note:\n    fields:\n      id: { type: uuid, primary: true }\n";
    const HOLE: &str = "holes:\n  notify:\n    kind: trigger\n    on: Order\n    events: [update]\n    writes: [Note]\n";

    fn kinds(old: &str, new: &str) -> Vec<(String, Data)> {
        let (a, d) = load(old);
        assert!(d.is_empty(), "{d:#?}");
        let (b, d) = load(new);
        assert!(d.is_empty(), "{d:#?}");
        plan(&a, old, &b, new)
            .changes
            .into_iter()
            .map(|c| (c.kind.to_string(), c.data))
            .collect()
    }

    #[test]
    fn adding_a_trigger_is_safe() {
        let got = kinds(FILE, &format!("{FILE}{HOLE}"));
        assert_eq!(got, [("add_hole".to_string(), Data::Safe)]);
    }

    #[test]
    fn changing_when_it_fires_is_breaking() {
        let old = format!("{FILE}{HOLE}");
        for new in [
            old.replace("[update]", "[insert, update]"),
            old.replace("on: Order", "on: Note")
                .replace("writes: [Note]", "writes: [Order]"),
            old.replace(
                "events: [update]",
                "events: [update]\n    when: \"status == 'paid'\"",
            ),
        ] {
            let got = kinds(&old, &new);
            assert!(
                got.contains(&("change_trigger".to_string(), Data::Breaking)),
                "{got:?}"
            );
        }
    }

    #[test]
    fn removing_a_trigger_is_breaking_and_says_how_to_drop_it() {
        let old = format!("{FILE}{HOLE}");
        let (a, _) = load(&old);
        let (b, _) = load(FILE);
        let p = plan(&a, &old, &b, FILE);
        let c = p
            .changes
            .iter()
            .find(|c| c.kind == "remove_hole")
            .expect("remove_hole");
        assert_eq!(c.data, Data::Breaking);
        let hint = format!("{:?}", c.hint);
        assert!(
            hint.contains(
                "drop trigger \\\"svetra_notify_update\\\" on \\\"public\\\".\\\"order\\\";"
            ),
            "{hint}"
        );
    }
}
