use svetra::report::validate;

/// Validate a one-entity file with `extra` appended, returning (code, message) pairs.
fn run(constraint: &str, policy_when: &str) -> Vec<(String, String)> {
    let src = format!(
        "svetra: \"0.1\"\nsystem: App\nentities:\n  Org:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      admin: {{ type: user, required: true }}\n\
         \n  Doc:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      org: {{ type: ref, entity: Org, required: true }}\n      owner: {{ type: user }}\n\
         \x20     status: {{ type: enum, values: [draft, live] }}\n      size: {{ type: int }}\n      created_at: {{ type: timestamp, auto: now }}\n\
         \x20   constraints:\n      c: \"{constraint}\"\n    indexes:\n      - [org]\n\
         policies:\n  org_rw:\n    on: Org\n    allow: [select, insert]\n    when: \"admin == auth.user\"\n\
         \x20 doc_rw:\n    on: Doc\n    allow: [select, insert]\n    when: \"{policy_when}\"\n"
    );
    validate(&src)
        .into_iter()
        .map(|d| (d.code.id().to_string(), d.message))
        .collect()
}

fn ok(constraint: &str, when: &str) {
    let found = run(constraint, when);
    assert!(found.is_empty(), "{constraint} / {when}: {found:?}");
}

fn svt003(constraint: &str, when: &str, needle: &str) {
    let found = run(constraint, when);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "SVT003");
    assert!(
        found[0].1.contains(needle),
        "expected `{needle}` in {:?}",
        found[0].1
    );
}

#[test]
fn valid_expressions_pass() {
    ok(
        "size >= 0 and size < 1000 * 1000",
        "owner == auth.user or org.admin == auth.user",
    );
    ok("status in ['draft', 'live']", "status == 'live'");
    ok("created_at > '2020-01-01' or owner == null", "true");
    ok("not (size == null)", "org.admin != null");
}

#[test]
fn constraints_cannot_reach_other_tables_or_the_user() {
    svt003("org.admin != null", "true", "own entity");
    svt003("owner == auth.user", "true", "only be used in policies");
}

#[test]
fn type_errors_are_reported_as_sentences() {
    svt003("size + 1", "true", "must be true or false");
    svt003("size == 'big'", "true", "Can't compare a number with text");
    svt003("status == 'publshed'", "true", "has no value 'publshed'");
    svt003("size > 0 and status", "true", "`and`/`or` needs true/false");
    svt003("nope > 0", "true", "Doc has no field `nope`");
}

#[test]
fn policy_paths_are_limited_to_one_hop_through_a_reference() {
    svt003("true", "size.admin == auth.user", "not a reference");
    svt003("true", "org.admin.x == 1", "more than one reference deep");
}

#[test]
fn parse_errors_point_at_the_character() {
    svt003("size = 1", "true", "at character 6");
}
