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

/// A document with a status, readable by everyone signed in; `policies` come after it.
fn docs(policies: &str) -> String {
    format!(
        "svetra: \"0.1\"\nsystem: App\nauth: {{ sign_in: [email] }}\nentities:\n  Doc:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user, required: true }}\n      title: {{ type: text, required: true }}\n      note: {{ type: text }}\n      status: {{ type: enum, values: [draft, sent, done] }}\n    indexes:\n      - [owner]\npolicies:\n  see: {{ on: Doc, allow: [select], when: \"true\" }}\n{policies}"
    )
}

#[test]
fn only_compiles_into_a_guard_that_refuses_with_42501() {
    let src = docs(
        "  decide:\n    on: Doc\n    allow: [update]\n    when: \"status == 'sent'\"\n    check: \"status == 'done'\"\n    only: [status]\n",
    );
    let sql = schema(&doc(&src));
    assert!(
        sql.contains("-- Guards\ncreate schema if not exists \"svetra\";"),
        "{sql}"
    );
    assert!(
        sql.contains("create function \"svetra\".\"guard_doc\"() returns trigger"),
        "{sql}"
    );
    // Only the API's roles are checked: the service role and hole bodies are exempt.
    assert!(
        sql.contains("if current_user not in ('anon', 'authenticated') then return new; end if;"),
        "{sql}"
    );
    // Title is left out of `only`, so it is locked unless a policy that frees it covers the person.
    assert!(
        sql.contains("if new.\"title\" is distinct from old.\"title\" and (true) then"),
        "{sql}"
    );
    assert!(sql.contains("errcode = '42501'"), "{sql}");
    // The status is in `only`, so there is no block for it.
    assert!(!sql.contains("new.\"status\" is distinct"), "{sql}");
    assert!(
        sql.contains("create trigger \"svetra_guard\" before update on \"public\".\"doc\""),
        "{sql}"
    );
    // A design with no `only` has no guard at all.
    let plain = schema(&doc(&docs(
        "  edit: { on: Doc, allow: [update], when: \"true\" }\n",
    )));
    assert!(
        !plain.contains("Guards") && !plain.contains("svetra_guard"),
        "{plain}"
    );
}

#[test]
fn a_policy_without_only_frees_every_field_for_the_people_it_covers() {
    let src = docs(
        "  mine:\n    on: Doc\n    allow: [update]\n    when: \"owner == auth.user\"\n  decide:\n    on: Doc\n    allow: [update]\n    when: \"status == 'sent'\"\n    only: [status]\n",
    );
    let sql = schema(&doc(&src));
    // `mine` leaves the title free, so the guard lets its people change it.
    assert!(
        sql.contains("and (not (coalesce(current_user = 'authenticated' and (old.\"owner\" = (select auth.uid())), false)))"),
        "{sql}"
    );
}

#[test]
fn a_restrictive_only_locks_a_field_whatever_the_permissive_policies_say() {
    let src = docs(
        "  mine: { on: Doc, allow: [update], when: \"owner == auth.user\" }\n  done_is_done:\n    on: Doc\n    allow: [update]\n    restrictive: true\n    when: \"status == 'done'\"\n    only: [note]\n",
    );
    let sql = schema(&doc(&src));
    assert!(sql.contains("(coalesce(current_user = 'authenticated' and (old.\"status\" = 'done'), false)) or not ("), "{sql}");
    let text = explain(&doc(&src), &[]);
    assert!(
        text.contains(", and only when its status is done (and then only its note can change)"),
        "{text}"
    );
}

#[test]
fn bad_only_is_reported() {
    let on_select = docs("  p: { on: Doc, allow: [select], when: \"true\", only: [title] }\n");
    assert!(
        codes(&on_select).contains(&"SVT035".to_string()),
        "{:?}",
        codes(&on_select)
    );
    let unknown = docs("  p: { on: Doc, allow: [update], when: \"true\", only: [nope, id] }\n");
    let found: Vec<_> = validate(&unknown)
        .into_iter()
        .filter(|d| d.code.id() == "SVT002")
        .collect();
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found[0].message.contains("no field `nope`") && found[1].message.contains("primary key")
    );
    let empty = docs("  p: { on: Doc, allow: [update], when: \"true\", only: [] }\n");
    assert!(
        codes(&empty).contains(&"SVT029".to_string()),
        "{:?}",
        codes(&empty)
    );
}

/// The locked field beside a transition the rules allow is what only the guard can refuse. The
/// expectation comes from the policies, not from the trigger.
#[test]
fn the_tests_expect_a_locked_field_beside_an_allowed_transition_to_be_refused() {
    let src = docs(
        "  decide:\n    on: Doc\n    allow: [update]\n    when: \"owner == auth.user and status == 'draft'\"\n    check: \"owner == auth.user and status == 'sent'\"\n    only: [status]\n",
    );
    let t = tests(&doc(&src));
    // Row a: alice owns it and it is a draft. Sending it is allowed; retitling at the same time isn't.
    assert!(
        t.contains("'alice can set the status of Doc a to sent'"),
        "{t}"
    );
    assert!(
        t.contains("alice can''t set the status of Doc a to sent and change the title"),
        "{t}"
    );
    assert!(
        t.contains("alice can''t set the status of Doc a to sent and change the note"),
        "{t}"
    );
    // On its own, the title is refused too, by the rules (the status didn't move).
    assert!(t.contains("alice can''t change the title of Doc a"), "{t}");
}

#[test]
fn a_policy_that_frees_what_another_limits_is_warned() {
    let src = docs(
        "  mine: { on: Doc, allow: [update], when: \"owner == auth.user\" }\n  decide:\n    on: Doc\n    allow: [update]\n    when: \"owner == auth.user and status == 'sent'\"\n    only: [status]\n",
    );
    let found: Vec<_> = validate(&src)
        .into_iter()
        .filter(|d| d.code.id() == "SVT036")
        .collect();
    assert!(!found.is_empty(), "{:?}", codes(&src));
    assert!(
        found[0].message.contains("through `mine`")
            && found[0].message.contains("the `only` of `decide`"),
        "{}",
        found[0].message
    );
    // Covering different people is no overlap.
    let apart = docs(
        "  mine: { on: Doc, allow: [update], when: \"owner == auth.user and status == 'draft'\" }\n  decide:\n    on: Doc\n    allow: [update]\n    when: \"owner == auth.user and status == 'sent'\"\n    only: [status]\n",
    );
    assert!(
        !codes(&apart).contains(&"SVT036".to_string()),
        "{:?}",
        codes(&apart)
    );
}

#[test]
fn explain_says_which_fields_an_update_may_change() {
    let src = docs(
        "  decide: { on: Doc, allow: [update], when: \"status == 'sent'\", only: [status, note] }\n",
    );
    let text = explain(&doc(&src), &[]);
    assert!(
        text.contains("They can change only its status and note."),
        "{text}"
    );
}
