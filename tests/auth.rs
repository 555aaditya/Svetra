use svetra::load::load;
use svetra::report::validate;

const APP: &str = "svetra: \"0.1\"\nsystem: Clinic\nentities:\n  Visit:\n    fields:\n      id: { type: uuid, primary: true }\n      patient: { type: user, required: true }\n    indexes:\n      - [patient]\npolicies:\n  own:\n    on: Visit\n    allow: [select, insert]\n    when: \"patient == auth.user\"\n";

fn codes(extra: &str) -> Vec<(String, String)> {
    validate(&format!("{APP}{extra}"))
        .into_iter()
        .map(|d| (d.code.id().to_string(), d.message))
        .collect()
}

#[test]
fn a_full_auth_section_loads_with_no_findings() {
    let extra = "auth:\n  sign_in: [email, google]\n  signup: invite_only\n  confirm_email: true\n  password: { min_length: 12, require: lower_upper_letters_digits }\nroles:\n  admin: { description: \"Runs the clinic\", mfa: true }\n";
    assert!(codes(extra).is_empty(), "{:?}", codes(extra));
    let (doc, _) = load(&format!("{APP}{extra}"));
    let auth = doc.auth.unwrap();
    assert!(auth.email() && auth.invite_only() && auth.confirms_email());
    assert_eq!(auth.min_length(), 12);
    assert_eq!(auth.requirements(), "lower_upper_letters_digits");
    assert!(doc.roles[0].mfa);
}

#[test]
fn defaults_are_safe_ones() {
    let (doc, _) = load(&format!("{APP}auth:\n  sign_in: [github]\n"));
    let auth = doc.auth.unwrap();
    assert!(!auth.email() && !auth.invite_only() && auth.confirms_email());
    assert_eq!(auth.min_length(), 10);
    assert_eq!(auth.requirements(), "");
}

#[test]
fn no_auth_section_is_svt009_info() {
    let found = codes("");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "SVT009");
    assert!(found[0].1.contains("passwords of 6 characters"));
}

#[test]
fn bad_sign_in_settings_are_svt008() {
    for (extra, needle) in [
        ("auth:\n  sign_in: []\n", "no sign-in method"),
        (
            "auth:\n  sign_in: [email, myspace]\n",
            "`myspace` isn't a sign-in method",
        ),
        ("auth:\n  sign_in: [email, email]\n", "listed twice"),
        (
            "auth:\n  sign_in: [email]\n  signup: closed\n",
            "`closed` isn't a signup setting",
        ),
        (
            "auth:\n  sign_in: [email]\n  password: { min_length: 6 }\n",
            "6 characters are too easy",
        ),
        (
            "auth:\n  sign_in: [email]\n  password: { require: digits }\n",
            "`digits` isn't a password rule",
        ),
        (
            "auth:\n  sign_in: [google]\n  password: { min_length: 12 }\n",
            "only applies to email sign-in",
        ),
        (
            "auth:\n  sign_in: [google]\n  confirm_email: false\n",
            "`auth.confirm_email` only applies",
        ),
    ] {
        let found = codes(extra);
        assert_eq!(found.len(), 1, "{extra}: {found:?}");
        assert_eq!(found[0].0, "SVT008", "{extra}");
        assert!(found[0].1.contains(needle), "{extra}: {:?}", found[0].1);
    }
}

#[test]
fn structural_mistakes_use_the_usual_codes() {
    assert_eq!(codes("auth:\n  signup: open\n")[0].0, "SVT028");
    assert_eq!(
        codes("auth:\n  sign_in: [email]\n  sso: true\n")[0].0,
        "SVT020"
    );
    assert_eq!(
        codes("auth:\n  sign_in: [email]\n  password: { min_length: twelve }\n")[0].0,
        "SVT029"
    );
    assert_eq!(
        codes("auth:\n  sign_in: [email]\nroles:\n  admin: { mfa: yes please }\n")[0].0,
        "SVT029"
    );
}

#[test]
fn mfa_roles_need_a_second_factor_in_sql_and_in_the_access_tests() {
    let src = include_str!("fixtures/clinic.yaml");
    let (doc, diags) = load(src);
    assert!(diags.is_empty(), "{diags:#?}");
    let sql = svetra::compile::sql::schema(&doc);
    assert_eq!(sql, include_str!("golden/clinic.sql"));
    assert!(sql.contains(
        "and (g.\"role\" not in ('admin') or coalesce((select auth.jwt() ->> 'aal'), '') = 'aal2')"
    ));
    let tests = svetra::compile::access_tests::tests(&doc);
    assert_eq!(tests, include_str!("golden/clinic.test.sql"));
    // The admin with a second factor runs in an aal2 session and gets the role;
    // the same grant without one gets only what any signed-in person gets.
    assert!(tests.contains("$svetra$, 'aal2'), 'allowed', 'admin can delete Visit b');"));
    assert!(tests.contains("'admin without a second factor can''t see Visit a');"));
    assert!(tests.contains("select plan(45);"));
    assert_eq!(
        svetra::compile::ts::types(&doc),
        include_str!("golden/clinic.ts")
    );
}

#[test]
fn roles_without_mfa_compile_as_before() {
    let src = include_str!("fixtures/bookings.yaml");
    let (doc, _) = load(src);
    assert!(!svetra::compile::sql::schema(&doc).contains("aal"));
}
