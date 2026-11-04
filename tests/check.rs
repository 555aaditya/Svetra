use svetra::check::check;
use svetra::load::load;

/// Load then check, returning codes in order. Panics if loading itself failed.
fn codes(src: &str) -> Vec<&'static str> {
    let (doc, load_diags) = load(src);
    assert!(load_diags.is_empty(), "{load_diags:#?}");
    check(&doc).iter().map(|d| d.code.id()).collect()
}

const HEAD: &str = "svetra: \"0.1\"\nsystem: App\n";
const OK_POLICY: &str =
    "policies:\n  a_rw:\n    on: A\n    allow: [select, insert]\n    when: \"true\"\n";

#[test]
fn spec_example_has_no_findings() {
    assert!(codes(include_str!("fixtures/finledger.yaml")).is_empty());
}

#[test]
fn exposed_entity_without_policy_is_svt001_and_svt004() {
    let src =
        format!("{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n");
    assert_eq!(codes(&src), ["SVT001", "SVT004"]);
}

#[test]
fn private_entity_needs_no_policy_but_a_policy_on_it_warns() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    private: true\n    fields:\n      id: {{ type: uuid, primary: true }}\n{OK_POLICY}"
    );
    assert_eq!(codes(&src), ["SVT027"]);
}

#[test]
fn bad_names_are_svt021() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      Bad: {{ type: text }}\n{OK_POLICY}"
    );
    assert_eq!(codes(&src), ["SVT021"]);
}

#[test]
fn shared_names_across_policies_and_holes_are_svt024() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n{OK_POLICY}holes:\n  a_rw:\n    kind: function\n"
    );
    assert_eq!(codes(&src), ["SVT024"]);
}

#[test]
fn primary_key_count_is_svt022() {
    let src =
        format!("{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid }}\n{OK_POLICY}");
    assert_eq!(codes(&src), ["SVT022"]);
}

#[test]
fn unknown_fields_in_indexes_are_svt026() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n    indexes:\n      - [nope]\n{OK_POLICY}"
    );
    assert_eq!(codes(&src), ["SVT026"]);
}

#[test]
fn dangling_references_are_svt002() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      b: {{ type: ref, entity: B }}\n    indexes:\n      - [b]\n{OK_POLICY}\
         holes:\n  h:\n    kind: function\n    input:\n      x: A.nope\n    writes: [C]\n"
    );
    assert_eq!(codes(&src), ["SVT002", "SVT002", "SVT002"]);
}

#[test]
fn unindexed_reference_is_svt010_info() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      parent: {{ type: ref, entity: A }}\n{OK_POLICY}"
    );
    assert_eq!(codes(&src), ["SVT010"]);
}

#[test]
fn float_money_is_svt005() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      unit_price: {{ type: float }}\n      ratio: {{ type: float }}\n{OK_POLICY}"
    );
    assert_eq!(codes(&src), ["SVT005"]);
}

#[test]
fn unindexed_user_field_is_svt010_info() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user }}\n{OK_POLICY}"
    );
    assert_eq!(codes(&src), ["SVT010"]);
}

#[test]
fn anon_only_access_is_svt007_but_anyone_is_fine() {
    let entity = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user }}\n    indexes:\n      - [owner]\n"
    );
    let own = "  own:\n    on: A\n    allow: [select, insert]\n    when: \"owner == auth.user\"\n";
    let public =
        "  public_read:\n    on: A\n    allow: [select]\n    when: \"true\"\n    as: anon\n";
    // Signed-in people only see their own rows, but logged-out visitors see all of them.
    assert_eq!(
        codes(&format!("{entity}policies:\n{own}{public}")),
        ["SVT007"]
    );
    assert!(
        codes(&format!(
            "{entity}policies:\n{own}{}",
            public.replace("as: anon", "as: anyone")
        ))
        .is_empty()
    );
    // A signed-in policy with the same condition covers it.
    let same = "  all_signed_in:\n    on: A\n    allow: [select]\n    when: \"true\"\n";
    assert!(codes(&format!("{entity}policies:\n{own}{public}{same}")).is_empty());
}
