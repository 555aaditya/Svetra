use svetra::compile::access_tests::tests;
use svetra::load::load;

fn generate(src: &str) -> String {
    let (doc, diags) = load(src);
    assert!(diags.is_empty(), "{diags:#?}");
    tests(&doc)
}

/// Golden files are compared byte for byte. Each was run with pgTAP on PostgreSQL 16 against
/// the fixture's compiled schema, with every check passing, and with a deliberately broken
/// policy making exactly the expected checks fail. Rerun both after regenerating one.
#[test]
fn access_tests_match_golden_files() {
    for (src, golden) in [
        (
            include_str!("fixtures/finledger.yaml"),
            include_str!("golden/finledger.test.sql"),
        ),
        (
            include_str!("fixtures/posts.yaml"),
            include_str!("golden/posts.test.sql"),
        ),
        (
            include_str!("fixtures/bookings.yaml"),
            include_str!("golden/bookings.test.sql"),
        ),
    ] {
        assert_eq!(generate(src), golden);
    }
}

#[test]
fn expected_results_follow_the_rules() {
    let t = generate(include_str!("fixtures/bookings.yaml"));
    // 7 test-data inserts; 6 people x 2 entities x 8 checks; alice granting herself a role; and
    // 5 signed-in people x 2 bookings x 2 fields (customer, location) changed by an update.
    assert!(t.contains("select plan(124);"));
    for name in [
        "alice can see Booking a",
        "alice can't see Booking b",
        "alice can create Booking c",
        "alice can't create Booking d",
        "alice can't delete Booking a",
        "anon can't see Location a",
        "admin can delete Booking b",
        "admin can't create Booking c",
        "staff at Location a can change Booking a",
        "staff at Location a can't change Booking b",
        "staff at Location a can see Location a",
        "staff at Location a can't see Location b",
        "carol can't see Booking a",
        "alice can't give herself the admin role",
        "alice can't set the customer of Booking a to bob",
        "carol can't set the customer of Booking a to themselves",
        "staff at Location a can't point the location of Booking a at Location b",
        "staff at Location a can set the customer of Booking a to themselves",
        "admin can point the location of Booking b at Location a",
    ] {
        assert!(
            t.contains(&format!("'{}');", name.replace('\'', "''"))),
            "{name}"
        );
    }
}

#[test]
fn checks_never_use_a_privileged_role() {
    let t = generate(include_str!("fixtures/bookings.yaml"));
    for line in t
        .lines()
        .filter(|l| l.starts_with("select is(svetra_test.try("))
    {
        assert!(
            line.contains("try('anon', null, ") || line.contains("try('authenticated', '5e7a"),
            "{line}"
        );
    }
    assert!(!t.contains("service_role"));
    assert!(t.trim_end().ends_with("rollback;"));
}

#[test]
fn signed_in_people_see_public_rows_and_private_entities_stay_closed() {
    let t = generate(include_str!("fixtures/posts.yaml"));
    assert!(t.contains("'alice can see Post b');"));
    assert!(t.contains("'anon can see Post a');"));
    assert!(t.contains("'anon can''t see AuditLog a');"));
    assert!(t.contains("'alice can''t create AuditLog c');"));
}

#[test]
fn a_second_ref_to_the_same_entity_points_at_the_other_row() {
    let t = generate(include_str!("fixtures/finledger.yaml"));
    // Transaction a: source is Account a, destination Account b, so `source != destination` holds.
    assert!(t.contains(
        "'5e7a0000-0000-4000-8001-00010000000a', '5e7a0000-0000-4000-8001-00010000000b'"
    ));
    // Both owners can see a transaction between their accounts, and carol, who owns neither, can't.
    assert!(t.contains("'alice can see Transaction a');"));
    assert!(t.contains("'bob can see Transaction a');"));
    assert!(t.contains("'carol can''t see Transaction a');"));
    // Only signed-in people reach transfer, and only through its public function.
    assert!(t.contains("'transfer'), true, 'signed-in people can call transfer');"));
    assert!(t.contains("'transfer'), false, 'anon can''t call transfer');"));
    assert!(t.contains("'svetra_holes', 'transfer'), false, 'signed-in people can''t call the inside of transfer directly');"));
}

#[test]
fn times_compare_like_postgres_and_unreadable_text_is_skipped() {
    let t = generate(
        r#"svetra: "0.1"
system: Times
entities:
  Event:
    fields:
      id:     { type: uuid, primary: true }
      starts: { type: timestamp, required: true }
      day:    { type: date, required: true }
policies:
  visible:       { on: Event, allow: [select], when: "true" }
  change_before: { on: Event, allow: [update], when: "starts < '2030-01-02 11:00'" }
  delete_on_day: { on: Event, allow: [delete], when: "day == '2030-01-02 23:00'" }
  create_future: { on: Event, allow: [insert], when: "starts > 'now'" }
"#,
    );
    // Event b starts at 2030-01-02 10:00 UTC: before 11:00, though `T` sorts after a space.
    assert!(t.contains("'alice can change Event b');"));
    // A date compared with text ignores the time of day, as Postgres does.
    assert!(t.contains("'alice can''t delete Event a');"));
    assert!(t.contains("'alice can delete Event b');"));
    // 'now' isn't a fixed time, so the check is skipped, never guessed.
    assert!(t.contains(
        "select skip('alice create Event c: a rule compares a time with text that isn''t an ISO time', 1);"
    ));
}
