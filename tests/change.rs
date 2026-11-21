use svetra::change::{Data, plan};
use svetra::load::load;
use svetra::report::validate;

const BASE: &str = r#"svetra: "0.1"
system: Shop
roles:
  admin: { description: "Runs the shop" }
entities:
  Customer:
    fields:
      id:    { type: uuid, primary: true }
      owner: { type: user, required: true }
      name:  { type: text, required: true }
      notes: { type: text }
      tier:  { type: enum, values: [free, paid] }
      spent: { type: money, precision: 12, scale: 2 }
    indexes:
      - [owner]
  Order:
    fields:
      id:       { type: uuid, primary: true }
      customer: { type: ref, entity: Customer, required: true }
policies:
  own_customers: { on: Customer, allow: [select, insert, update], when: "owner == auth.user" }
  own_orders:    { on: Order, allow: [select], when: "customer.owner == auth.user" }
"#;

/// The kinds, data classes and access directions of the changes from BASE to `new`.
fn kinds(new: &str) -> Vec<(String, Data, Option<&'static str>)> {
    let (a, d) = load(BASE);
    assert!(d.is_empty(), "{d:#?}");
    let (b, d) = load(new);
    assert!(d.is_empty(), "{d:#?}");
    plan(&a, BASE, &b, new)
        .changes
        .into_iter()
        .map(|c| (c.kind.to_string(), c.data, c.access))
        .collect()
}

fn has(found: &[(String, Data, Option<&'static str>)], kind: &str, data: Data) -> bool {
    found.iter().any(|(k, d, _)| k == kind && *d == data)
}

#[test]
fn no_change_is_no_change() {
    assert!(kinds(BASE).is_empty());
    // Reordering and reformatting are not changes: the models are compared, not the text.
    let moved = BASE.replace("      notes: { type: text }\n", "").replace(
        "      spent: { type: money, precision: 12, scale: 2 }\n",
        "      spent: { type: money, precision: 12, scale: 2 }\n      notes:   {  type: text  }\n",
    );
    assert!(kinds(&moved).is_empty(), "{:?}", kinds(&moved));
}

#[test]
fn data_changes_are_classified() {
    let cases: &[(&str, &str, &str, Data)] = &[
        (
            "      notes: { type: text }\n",
            "",
            "remove_field",
            Data::Destructive,
        ),
        (
            "      notes: { type: text }\n",
            "      notes: { type: text }\n      phone: { type: text }\n",
            "add_field",
            Data::Safe,
        ),
        (
            "      notes: { type: text }\n",
            "      notes: { type: text }\n      phone: { type: text, required: true }\n",
            "add_field",
            Data::Breaking,
        ),
        (
            "      notes: { type: text }\n",
            "      notes: { type: text, required: true }\n",
            "make_required",
            Data::Breaking,
        ),
        (
            "      name:  { type: text, required: true }",
            "      name:  { type: text }",
            "make_optional",
            Data::Safe,
        ),
        (
            "customer: { type: ref, entity: Customer, required: true }",
            "customer: { type: ref, entity: Customer, required: true, on_delete: cascade }",
            "change_on_delete",
            Data::Breaking,
        ),
        (
            "customer: { type: ref, entity: Customer, required: true }",
            "customer: { type: ref, entity: Customer, on_delete: set_null }",
            "change_on_delete",
            Data::Safe,
        ),
        (
            "values: [free, paid]",
            "values: [free, paid, gold]",
            "add_enum_value",
            Data::Safe,
        ),
        (
            "values: [free, paid]",
            "values: [free]",
            "remove_enum_value",
            Data::Breaking,
        ),
        (
            "precision: 12, scale: 2",
            "precision: 18, scale: 4",
            "change_type",
            Data::Safe,
        ),
        (
            "precision: 12, scale: 2",
            "precision: 12, scale: 0",
            "change_type",
            Data::Destructive,
        ),
        (
            "      notes: { type: text }\n",
            "      notes: { type: int }\n",
            "change_type",
            Data::Destructive,
        ),
        (
            "    indexes:\n      - [owner]\n",
            "    indexes:\n      - [owner]\n      - [name]\n",
            "add_index",
            Data::Safe,
        ),
        (
            "    indexes:\n      - [owner]\n",
            "    indexes:\n      - [owner]\n    unique:\n      - [name]\n",
            "add_unique",
            Data::Breaking,
        ),
        (
            "      tier:",
            "      code: { type: text, unique: true }\n      tier:",
            "add_field",
            Data::Safe,
        ),
        (
            "roles:\n  admin: { description: \"Runs the shop\" }\n",
            "",
            "remove_role",
            Data::Destructive,
        ),
    ];
    for (from, to, kind, data) in cases {
        let found = kinds(&BASE.replacen(from, to, 1));
        assert!(
            has(&found, kind, *data),
            "{kind} {data:?} for {from:?} -> {to:?}: {found:?}"
        );
    }
}

#[test]
fn removing_an_entity_is_destructive_and_adding_one_is_safe() {
    let without = BASE
        .replace("  Order:\n    fields:\n      id:       { type: uuid, primary: true }\n      customer: { type: ref, entity: Customer, required: true }\n", "")
        .replace("  own_orders:    { on: Order, allow: [select], when: \"customer.owner == auth.user\" }\n", "");
    assert!(has(&kinds(&without), "remove_entity", Data::Destructive));
    let with = BASE.replace("policies:", "  Note:\n    private: true\n    fields:\n      id: { type: uuid, primary: true }\npolicies:");
    assert!(has(&kinds(&with), "add_entity", Data::Safe));
}

#[test]
fn a_declared_rename_keeps_the_data_and_writes_the_migration() {
    let new = BASE
        .replace(
            "      notes: { type: text }",
            "      remarks: { type: text, was: notes }",
        )
        .replace("  Customer:\n", "  Client:\n    was: Customer\n")
        .replace("entity: Customer", "entity: Client")
        .replace("on: Customer", "on: Client");
    assert!(validate(&new).iter().all(|d| d.code.id() != "SVT030"));
    let (a, _) = load(BASE);
    let (b, _) = load(&new);
    let p = plan(&a, BASE, &b, &new);
    let kinds: Vec<&str> = p.changes.iter().map(|c| c.kind).collect();
    assert!(
        kinds.contains(&"rename_entity") && kinds.contains(&"rename_field"),
        "{kinds:?}"
    );
    assert!(
        p.changes.iter().all(|c| c.data != Data::Destructive),
        "{:?}",
        p.changes
    );
    // The ref to the renamed entity is the same type, not a change.
    assert!(!kinds.contains(&"change_type"), "{kinds:?}");
    // Tables first, then columns on the new table name.
    assert_eq!(
        p.renames,
        [
            "alter table \"public\".\"customer\" rename to \"client\";",
            "alter table \"public\".\"client\" rename column \"notes\" to \"remarks\";",
        ]
    );
}

#[test]
fn an_undeclared_rename_is_destructive_with_a_hint() {
    let new = BASE.replace(
        "      notes: { type: text }",
        "      remarks: { type: text }",
    );
    let (a, _) = load(BASE);
    let (b, _) = load(&new);
    let p = plan(&a, BASE, &b, &new);
    let c = p.changes.iter().find(|c| c.kind == "remove_field").unwrap();
    assert_eq!(c.data, Data::Destructive);
    assert!(
        c.hint
            .as_deref()
            .unwrap()
            .contains("Add `was: notes` to remarks")
    );
    assert!(p.renames.is_empty());
}

#[test]
fn wider_and_narrower_access_name_the_people() {
    let wider = BASE.replace(
        "  own_orders:",
        "  everyone_sees_customers: { on: Customer, allow: [select], when: \"true\" }\n  own_orders:",
    );
    let (a, _) = load(BASE);
    let (b, _) = load(&wider);
    let p = plan(&a, BASE, &b, &wider);
    let line = p
        .changes
        .iter()
        .find(|c| {
            c.access == Some("wider")
                && c.message.starts_with("A signed-in person who owns nothing")
        })
        .unwrap();
    assert_eq!(
        line.message,
        "A signed-in person who owns nothing can now see all customers (was: none)."
    );
    assert_eq!(line.because, ["Signed-in people can see customers."]);
    assert!(p.needs_yes().contains(&"wider_access"));

    let narrower = BASE.replace(
        "allow: [select, insert, update], when",
        "allow: [insert, update], when",
    );
    let (b, _) = load(&narrower);
    let p = plan(&a, BASE, &b, &narrower);
    assert!(p.changes.iter().any(|c| c.access == Some("narrower")
        && c.message == "A signed-in person can no longer see customers (was: some)."));
    assert!(!p.needs_yes().contains(&"narrower_access"));
}

#[test]
fn was_must_name_something_gone() {
    // Statically: the old name is still here.
    let src = BASE.replace(
        "      notes: { type: text }",
        "      notes: { type: text }\n      remarks: { type: text, was: notes }",
    );
    let d: Vec<_> = validate(&src)
        .into_iter()
        .filter(|d| d.code.id() == "SVT030")
        .collect();
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0].path, "entities.Customer.fields.remarks.was");
    // In a plan: the old design never had it (SVT030), or already has the new name (SVT031).
    let (a, _) = load(BASE);
    for (from, to, code) in [
        (
            "      notes: { type: text }",
            "      remarks: { type: text, was: memo }",
            "SVT030",
        ),
        (
            "      notes: { type: text }",
            "      notes: { type: text, was: memo }",
            "SVT031",
        ),
    ] {
        let new = BASE.replace(from, to);
        let (b, _) = load(&new);
        let p = plan(&a, BASE, &b, &new);
        assert_eq!(
            p.diagnostics
                .iter()
                .map(|d| d.code.id())
                .collect::<Vec<_>>(),
            [code],
            "{to}"
        );
    }
}

#[test]
fn plan_waits_for_a_yes_then_writes_the_rename_once() {
    let dir = std::env::temp_dir().join(format!("svetra-plan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("old.yaml"), BASE).unwrap();
    let new = BASE
        .replace(
            "      notes: { type: text }",
            "      remarks: { type: text, was: notes }",
        )
        .replace("      tier:  { type: enum, values: [free, paid] }\n", "");
    std::fs::write(dir.join("svetra.yaml"), &new).unwrap();
    let run = |extra: &[&str]| {
        let mut args = vec![
            "plan",
            dir.join("svetra.yaml").to_str().unwrap().to_string().leak() as &str,
            "--from",
            dir.join("old.yaml").to_str().unwrap().to_string().leak(),
            "--at",
            "20260101000000",
        ];
        args.extend_from_slice(extra);
        std::process::Command::new(env!("CARGO_BIN_EXE_svetra"))
            .args(&args)
            .output()
            .unwrap()
    };
    let migration = dir.join("supabase/migrations/20260101000000_svetra_rename.sql");
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("--accept remove_field"));
    assert!(!migration.exists(), "nothing is written before the yes");

    let out = run(&["--accept", "remove_field"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        std::fs::read_to_string(&migration).unwrap(),
        "-- Written by svetra plan: renames declared with `was`, before supabase db diff.\nalter table \"public\".\"customer\" rename column \"notes\" to \"remarks\";\n"
    );
    // Running it again doesn't write a second migration.
    let out = run(&["--accept", "remove_field", "--at", "20260102000000"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        std::fs::read_dir(dir.join("supabase/migrations"))
            .unwrap()
            .count(),
        1
    );

    // diff only reads: it never fails on what it finds, and has no --accept.
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args([
            "diff",
            dir.join("svetra.yaml").to_str().unwrap(),
            "--from",
            dir.join("old.yaml").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .contains("destructive  remove Customer.tier: every value is deleted")
    );
}

/// BASE with an update rule that has `only` (or not), to compare column rules.
fn with_only(only: &str) -> String {
    BASE.replace(
        "  own_orders:",
        &format!(
            "  edit_customers: {{ on: Customer, allow: [update], when: \"owner == auth.user\"{only} }}\n  own_orders:"
        ),
    )
}

/// The access lines (direction, message) on the `edit_customers` rule from `old` to `new`.
fn column_lines(old: &str, new: &str) -> Vec<(&'static str, String)> {
    let (a, d) = load(old);
    assert!(d.is_empty(), "{d:#?}");
    let (b, d) = load(new);
    assert!(d.is_empty(), "{d:#?}");
    plan(&a, old, &b, new)
        .changes
        .into_iter()
        .filter(|c| c.path == "policies.edit_customers" && c.access.is_some())
        .map(|c| (c.access.unwrap(), c.message))
        .collect()
}

fn needs_yes(old: &str, new: &str) -> Vec<&'static str> {
    let (a, _) = load(old);
    let (b, _) = load(new);
    plan(&a, old, &b, new).needs_yes()
}

#[test]
fn gaining_only_is_narrower() {
    let (old, new) = (with_only(""), with_only(", only: [name]"));
    assert_eq!(
        column_lines(&old, &new),
        [(
            "narrower",
            "Rule edit_customers now lets an update change only name of customers (was: every field).".to_string()
        )]
    );
    assert!(!needs_yes(&old, &new).contains(&"narrower_access"));
    assert!(!needs_yes(&old, &new).contains(&"wider_access"));
}

#[test]
fn losing_only_is_wider_and_plan_wants_a_yes() {
    let (old, new) = (with_only(", only: [name]"), with_only(""));
    assert_eq!(
        column_lines(&old, &new),
        [(
            "wider",
            "Rule edit_customers now lets an update change every field of customers (was: only name).".to_string()
        )]
    );
    assert!(needs_yes(&old, &new).contains(&"wider_access"));
}

#[test]
fn a_field_added_to_only_is_wider_and_removed_is_narrower() {
    let one = with_only(", only: [name]");
    let two = with_only(", only: [name, notes]");
    assert_eq!(
        column_lines(&one, &two),
        [(
            "wider",
            "Rule edit_customers now also lets an update change notes.".to_string()
        )]
    );
    assert!(needs_yes(&one, &two).contains(&"wider_access"));
    assert_eq!(
        column_lines(&two, &one),
        [(
            "narrower",
            "Rule edit_customers no longer lets an update change notes.".to_string()
        )]
    );
    assert!(!needs_yes(&two, &one).contains(&"wider_access"));
}

#[test]
fn reordering_only_or_renaming_a_listed_field_is_not_a_column_change() {
    let old = with_only(", only: [name, notes]");
    assert!(column_lines(&old, &with_only(", only: [notes, name]")).is_empty());
    let new = old
        .replace(
            "name:  { type: text, required: true }",
            "title: { type: text, required: true, was: name }",
        )
        .replace("only: [name, notes]", "only: [title, notes]");
    assert!(new.contains("was: name"));
    assert!(column_lines(&old, &new).is_empty());
}

#[test]
fn turning_audit_on_or_off_is_listed_and_safe_for_data() {
    let on = BASE.replace("  Customer:\n", "  Customer:\n    audit: true\n");
    assert!(has(&kinds(&on), "add_audit", Data::Safe), "{:?}", kinds(&on));
    // The other way round: from an audited design to the plain one.
    let (a, _) = load(&on);
    let (b, _) = load(BASE);
    let p = plan(&a, &on, &b, BASE);
    let c = p.changes.iter().find(|c| c.kind == "remove_audit").expect("listed");
    assert_eq!((c.data, c.path.as_str()), (Data::Safe, "entities.Customer"));
    assert!(c.hint.as_deref().unwrap_or("").contains("stay"));
}

#[test]
fn turning_audit_on_for_a_hole_is_listed_by_hole() {
    let hole = |audit: &str| {
        format!("{BASE}holes:\n  close:\n    kind: function\n    writes: [Order]\n{audit}")
    };
    let (plain, audited) = (hole(""), hole("    audit: true\n"));
    let (a, d) = load(&plain);
    assert!(d.is_empty(), "{d:#?}");
    let (b, d) = load(&audited);
    assert!(d.is_empty(), "{d:#?}");
    let found: Vec<_> = plan(&a, &plain, &b, &audited)
        .changes
        .into_iter()
        .map(|c| (c.kind, c.path))
        .collect();
    assert_eq!(found, [("add_audit", "holes.close".to_string())]);
}
