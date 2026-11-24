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

#[test]
fn grants_load_and_must_name_declared_roles() {
    let src = include_str!("fixtures/teams.yaml");
    assert!(codes(src).is_empty(), "{:?}", codes(src));
    let (doc, _) = load(src);
    let names: Vec<&str> = doc.holes[0]
        .grants
        .iter()
        .map(|g| g.name.as_str())
        .collect();
    assert_eq!(names, ["member"]);

    let bad = src.replacen("grants: [member]", "grants: [owner]", 1);
    let found = codes(&bad);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "SVT006");
    assert!(found[0].1.contains("`owner`"), "{found:?}");
    let d = validate(&bad)
        .into_iter()
        .find(|d| d.code.id() == "SVT006")
        .unwrap();
    assert_eq!((d.line, d.path.as_str()), (39, "holes.start_team.grants"));
}

#[test]
fn a_hole_that_grants_one_role_cannot_reach_the_others() {
    let (doc, _) = load(include_str!("fixtures/teams.yaml"));
    let sql = svetra::compile::sql::schema(&doc);
    assert!(sql.contains(
        "create policy \"svetra_hole_add_member\" on \"svetra\".\"role_grants\" for all to \"svetra_hole_add_member\" using (\"role\" in ('member')) with check (\"role\" in ('member'));"
    ));
    let tests = svetra::compile::access_tests::tests(&doc);
    assert!(tests.contains("'add_member can''t give the admin role'"));
    assert!(tests.contains("'add_member can give the member role'"));
}

fn compile_ledger_with_body(dir_name: &str, body: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!("svetra-body-{dir_name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("supabase/holes")).unwrap();
    std::fs::write(
        dir.join("svetra.yaml"),
        include_str!("fixtures/finledger.yaml"),
    )
    .unwrap();
    std::fs::write(dir.join("supabase/holes/transfer.sql"), body).unwrap();
    std::process::Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(["compile", dir.join("svetra.yaml").to_str().unwrap()])
        .output()
        .unwrap()
}

/// The body an agent wrote in the evaluation: a whole function, `begin … end` with no `;`. Svetra
/// puts its own begin and end around the body, so it left a stray `end` and a migration that
/// didn't parse. It is refused before anything is written, and the report names the body file.
#[test]
fn a_whole_function_body_is_refused_against_its_own_file() {
    let body = "begin\n  update \"public\".\"account\" set \"balance\" = \"balance\" - p_amount\n   where \"id\" = p_source;\n  if not found then\n    raise exception 'no such account' using errcode = '42501';\n  end if;\nend\n";
    let out = compile_ledger_with_body("whole", body);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("supabase/holes/transfer.sql:7:1: error SVT032"),
        "{text}"
    );
    assert!(
        text.contains("whole begin … end block with no `;`")
            && text.contains("Svetra already adds the outer begin and end"),
        "{text}"
    );
    assert!(
        text.contains("Delete the outer `begin` and `end`")
            && text.contains("Nothing was written."),
        "{text}"
    );
}

#[test]
fn a_body_must_end_with_a_semicolon_but_nested_blocks_and_comments_are_fine() {
    let refused = |body: &str| {
        let out = compile_ledger_with_body("semi", body);
        String::from_utf8_lossy(&out.stdout).contains("SVT032")
    };
    // Not statements: refused.
    assert!(refused("update \"public\".\"account\" set \"balance\" = 0"));
    assert!(refused("  perform 1;\n  if true then perform 2; end if"));
    // A nested block ending with `end;`, trailing comments and an empty body are all valid.
    assert!(!refused("declare v int;\nbegin\n  v := 1;\nend;\n"));
    assert!(!refused("  perform 1;\n-- done\n/* really done */\n"));
    assert!(!refused(""));
    assert!(!refused(include_str!(
        "fixtures/holes/finledger/transfer.sql"
    )));
}

#[test]
fn require_loads_and_checks() {
    let src = ledger("    require:\n      - \"amount > 0\"\n      - \"source != destination\"");
    assert!(codes(&src).is_empty(), "{:?}", codes(&src));
    let (doc, _) = load(&src);
    assert_eq!(
        doc.holes[0].require,
        ["amount > 0", "source != destination"]
    );
}

#[test]
fn require_is_checked_against_the_inputs() {
    for (cond, needle) in [
        ("balance > 0", "has no field `balance`"),
        ("amount", "must be true or false"),
        ("source.nope == auth.user", "has no field `nope`"),
    ] {
        let src = ledger(&format!("    require:\n      - \"{cond}\""));
        let found = codes(&src);
        assert!(
            found
                .iter()
                .any(|(c, m)| c == "SVT003" && m.contains(needle)),
            "{cond}: {found:?}"
        );
    }
}

#[test]
fn require_may_not_read_through_an_input_the_hole_doesnt_declare() {
    let src = ledger("    require:\n      - \"source.owner == auth.user\"").replace(
        "    reads: [Account]\n    writes: [Account, Transaction]",
        "    writes: [Transaction]",
    );
    let found = codes(&src);
    assert!(
        found
            .iter()
            .any(|(c, m)| c == "SVT003" && m.contains("`require` reads Account")),
        "{found:?}"
    );
}

#[test]
fn an_empty_or_misshapen_require_is_refused() {
    for tail in ["    require: []", "    require: \"amount > 0\""] {
        let found = codes(&ledger(tail));
        assert!(
            found.iter().any(|(c, _)| c == "SVT029"),
            "{tail}: {found:?}"
        );
    }
}

#[test]
fn require_is_refused_on_a_scheduled_or_triggered_hole() {
    let schedule = "svetra: \"0.1\"\nsystem: T\nentities:\n  Inv:\n    fields:\n      id: { type: uuid, primary: true }\nholes:\n  sweep:\n    kind: schedule\n    schedule: \"0 3 * * *\"\n    writes: [Inv]\n    require:\n      - \"true\"\n";
    let trigger = "svetra: \"0.1\"\nsystem: T\nentities:\n  Inv:\n    fields:\n      id: { type: uuid, primary: true }\nholes:\n  on_add:\n    kind: trigger\n    on: Inv\n    events: [insert]\n    writes: [Inv]\n    require:\n      - \"true\"\n";
    for src in [schedule, trigger] {
        let found = codes(src);
        assert!(
            found
                .iter()
                .any(|(c, m)| c == "SVT020" && m.contains("there are no inputs to check")),
            "{found:?}"
        );
    }
}
