use svetra::report::validate;

/// A bookings app with a global and a scoped role. `extra` is appended as more policies.
fn app(extra: &str) -> String {
    format!(
        "svetra: \"0.1\"\nsystem: Bookings\nroles:\n  admin: {{ description: \"Runs the business\" }}\n  staff: {{ description: \"Works at one location\", scope: Location }}\n\
         entities:\n  Location:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      name: {{ type: text, required: true }}\n\
         \x20 Booking:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      customer: {{ type: user, required: true }}\n      location: {{ type: ref, entity: Location, required: true }}\n    indexes:\n      - [customer]\n      - [location]\n\
         policies:\n  customers_own: {{ on: Booking, allow: [select, insert], when: \"customer == auth.user\" }}\n  admins_locations: {{ on: Location, allow: [select, insert, update], when: \"has_role('admin')\" }}\n{extra}"
    )
}

fn codes(src: &str) -> Vec<(String, String)> {
    validate(src)
        .into_iter()
        .map(|d| (d.code.id().to_string(), d.message))
        .collect()
}

#[test]
fn valid_global_and_scoped_role_checks() {
    let src = app(
        "  staff_local: { on: Booking, allow: [select, update], when: \"has_role('staff', location)\" }\n  staff_see_location: { on: Location, allow: [select], when: \"has_role('staff', id)\" }\n  admins_all: { on: Booking, allow: [select, update, delete], when: \"has_role('admin') or customer == auth.user\" }\n",
    );
    assert!(codes(&src).is_empty(), "{:?}", codes(&src));
}

#[test]
fn role_mistakes_are_svt006() {
    let cases = [
        ("has_role('owner')", "No role named `owner`"),
        ("has_role('admin', location)", "global role"),
        ("has_role('staff')", "needs a second argument"),
        ("has_role('staff', customer)", "held per Location"),
        ("has_role(admin)", "role name in quotes"),
    ];
    for (when, needle) in cases {
        let found = codes(&app(&format!(
            "  p: {{ on: Booking, allow: [select], when: \"{when}\" }}\n"
        )));
        assert_eq!(found.len(), 1, "{when}: {found:?}");
        assert_eq!(found[0].0, "SVT006", "{when}");
        assert!(found[0].1.contains(needle), "{when}: {}", found[0].1);
    }
}

#[test]
fn has_role_is_not_allowed_in_constraints() {
    let src = app("").replace("      name: { type: text, required: true }\n", "      name: { type: text, required: true }\n    constraints:\n      c: \"has_role('admin')\"\n");
    let found = codes(&src);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "SVT003");
    assert!(found[0].1.contains("only be used in policies"));
}

#[test]
fn has_role_compiles_and_explains() {
    let src = app(
        "  staff_local: { on: Booking, allow: [select], when: \"has_role('staff', location)\" }\n",
    );
    let (doc, diags) = svetra::load::load(&src);
    assert!(diags.is_empty());
    let sql = svetra::compile::sql::schema(&doc);
    assert!(
        sql.contains("using ((select \"svetra\".\"has_role\"('admin')));"),
        "{sql}"
    );
    assert!(
        sql.contains("using (\"svetra\".\"has_role\"('staff', \"location\"));"),
        "{sql}"
    );
    let text = svetra::understand::explain::explain(&doc, &[]);
    assert!(
        text.contains("when they have the staff role for its location"),
        "{text}"
    );
}
