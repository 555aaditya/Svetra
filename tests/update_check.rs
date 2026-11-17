use svetra::compile::access_tests::tests;
use svetra::compile::sql::schema;
use svetra::load::load;
use svetra::report::validate;
use svetra::understand::explain::explain;

fn doc(src: &str) -> svetra::model::Document {
    let (doc, diags) = load(src);
    assert!(diags.is_empty(), "{diags:#?}");
    doc
}

fn codes(src: &str) -> Vec<String> {
    validate(src)
        .into_iter()
        .map(|d| d.code.id().to_string())
        .collect()
}

/// An entity with a state field, readable and writable by anyone signed in.
fn doc_with(policies: &str) -> String {
    format!(
        "svetra: \"0.1\"\nsystem: App\nauth: {{ sign_in: [email] }}\nentities:\n  Doc:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user, required: true }}\n      mode: {{ type: enum, values: [one, two, three] }}\n      locked: {{ type: bool, required: true }}\npolicies:\n{policies}"
    )
}

#[test]
fn check_is_the_with_check_and_restrictive_is_as_restrictive() {
    let src = doc_with(
        "  see: { on: Doc, allow: [select], when: \"true\" }\n  edit:\n    on: Doc\n    allow: [update]\n    when: \"mode == 'one'\"\n    check: \"mode in ['one', 'two']\"\n  unlocked:\n    on: Doc\n    allow: [update]\n    restrictive: true\n    when: \"locked == false\"\n",
    );
    let sql = schema(&doc(&src));
    assert!(
        sql.contains("using (\"mode\" = 'one')\n  with check (\"mode\" in ('one', 'two'))"),
        "{sql}"
    );
    assert!(
        sql.contains(
            "create policy \"unlocked\" on \"public\".\"doc\" as restrictive\n  for update"
        ),
        "{sql}"
    );
    // Without a `check`, `when` is both, as before.
    let plain = schema(&doc(&doc_with(
        "  all: { on: Doc, allow: [select, update], when: \"mode == 'one'\" }\n",
    )));
    assert!(
        plain.contains("using (\"mode\" = 'one')\n  with check (\"mode\" = 'one')"),
        "{plain}"
    );
}

#[test]
fn a_check_only_makes_sense_on_update() {
    let src = doc_with(
        "  p: { on: Doc, allow: [select], when: \"true\", check: \"true\" }\n  q: { on: Doc, allow: [update], when: \"true\", check: \"mode == 'one'\" }\n",
    );
    let found: Vec<_> = validate(&src)
        .into_iter()
        .filter(|d| d.code.id() == "SVT033")
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, "policies.p.check");
    // The expression is type-checked like `when`.
    let bad = doc_with("  q: { on: Doc, allow: [update], when: \"true\", check: \"mode == 5\" }\n");
    assert!(
        codes(&bad).contains(&"SVT003".to_string()),
        "{:?}",
        codes(&bad)
    );
}

#[test]
fn restrictive_policies_never_let_anyone_in() {
    let only = doc_with(
        "  limit: { on: Doc, allow: [select], restrictive: true, when: \"locked == false\" }\n",
    );
    let found: Vec<_> = validate(&only)
        .into_iter()
        .filter(|d| d.code.id() == "SVT001")
        .collect();
    assert_eq!(found.len(), 1);
    assert!(
        found[0].message.contains("only restrictive policies"),
        "{}",
        found[0].message
    );
    // With a permissive policy beside it, the design is fine.
    let both = doc_with(
        "  see: { on: Doc, allow: [select], when: \"true\" }\n  limit: { on: Doc, allow: [select], restrictive: true, when: \"locked == false\" }\n",
    );
    assert!(!codes(&both).contains(&"SVT001".to_string()));
}

/// The old row passes one policy's `when` and the new row passes another's `check`. PostgreSQL
/// allows it (checked on PostgreSQL 16 in docs/update-check.md), so the generated tests expect it
/// to be allowed, and `validate` warns that no single policy does.
#[test]
fn an_update_two_policies_allow_only_together_is_warned_and_expected_allowed() {
    let src = doc_with(
        "  see: { on: Doc, allow: [select], when: \"true\" }\n  a: { on: Doc, allow: [update], when: \"mode == 'one'\", check: \"mode == 'one'\" }\n  b: { on: Doc, allow: [update], when: \"mode == 'two'\", check: \"mode == 'three'\" }\n",
    );
    let found: Vec<_> = validate(&src)
        .into_iter()
        .filter(|d| d.code.id() == "SVT034")
        .collect();
    // Two real hazards: a to three through b, and b to one through a.
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found[0]
            .message
            .contains("policy `a` allows it on the row as it is and `b` on the row afterwards"),
        "{}",
        found[0].message
    );
    assert!(
        found[1]
            .message
            .contains("policy `b` allows it on the row as it is and `a` on the row afterwards"),
        "{}",
        found[1].message
    );
    let t = tests(&doc(&src));
    // Row a is mode one: setting it to three passes a's `when` (old) and b's `check` (new).
    assert!(
        t.contains("alice can set the mode of Doc a to three"),
        "{t}"
    );
    // Setting it to two passes neither check.
    assert!(
        t.contains("alice can''t set the mode of Doc a to two"),
        "{t}"
    );
    // One policy that allows the same transition is not a hazard.
    let single = doc_with(
        "  see: { on: Doc, allow: [select], when: \"true\" }\n  a: { on: Doc, allow: [update], when: \"mode == 'one'\", check: \"mode in ['one', 'three']\" }\n",
    );
    assert!(!codes(&single).contains(&"SVT034".to_string()));
}

#[test]
fn a_restrictive_policy_narrows_what_the_tests_expect_and_how_explain_reads() {
    let src = doc_with(
        "  see: { on: Doc, allow: [select], when: \"true\" }\n  edit: { on: Doc, allow: [update], when: \"true\", check: \"true\" }\n  unlocked: { on: Doc, allow: [update], restrictive: true, when: \"locked == false\" }\n",
    );
    let d = doc(&src);
    let t = tests(&d);
    // Row a is locked (true), row b isn't: the restrictive policy blocks only the update of a.
    assert!(t.contains("alice can''t change Doc a"), "{t}");
    assert!(t.contains("alice can change Doc b"), "{t}");
    let text = explain(&d, &[]);
    assert!(
        text.contains("can change a doc when always")
            || text.contains("can change docs, and only when"),
        "{text}"
    );
    assert!(text.contains(", and only when "), "{text}");
}

#[test]
fn a_second_user_field_points_at_the_other_person() {
    let src = "svetra: \"0.1\"\nsystem: App\nauth: { sign_in: [email] }\nentities:\n  Doc:\n    fields:\n      id: { type: uuid, primary: true }\n      author: { type: user, required: true }\n      reviewer: { type: user, required: true }\n    constraints: {}\npolicies:\n  see: { on: Doc, allow: [select], when: \"author == auth.user or reviewer == auth.user\" }\n";
    let t = tests(&doc(src));
    // Row a: alice wrote it and bob reviews it. Row b: bob wrote it and alice reviews it.
    assert!(
        t.contains(
            "'5e7a0000-0000-4000-8000-000000000001', '5e7a0000-0000-4000-8000-000000000002'"
        ),
        "author alice, reviewer bob on row a:\n{t}"
    );
}
