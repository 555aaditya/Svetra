//! `svetra patch` (docs/patch.md): the text edits, byte for byte, then the command.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use svetra::patch::{Problem, apply, parse_ops};

const FILE: &str = r#"svetra: "0.1"
system: Shop

# What the shop keeps.
entities:
  Booking:
    fields:
      id:       { type: uuid, primary: true }
      customer: { type: user, required: true }   # who booked
      notes:    { type: text }
  # Comments are removable.
  Comment:
    fields:
      id:   { type: uuid, primary: true }
      body: { type: text }

policies:
  own_bookings: { on: Booking, allow: [select], when: "customer == auth.user" }
  all_comments: { on: Comment, allow: [select], when: "true" }
"#;

fn edit(src: &str, ops: &str) -> (String, Vec<Problem>) {
    let ops = parse_ops(ops).expect("ops");
    apply(src, &ops)
}

fn ok(src: &str, ops: &str) -> String {
    let (out, problems) = edit(src, ops);
    assert!(problems.is_empty(), "{problems:#?}");
    out
}

#[test]
fn set_replaces_a_value_in_place_and_keeps_the_comment() {
    let out = ok(
        FILE,
        r#"[{"op":"set","path":"entities.Booking.fields.customer.required","value":false}]"#,
    );
    assert_eq!(
        out,
        FILE.replace(
            "{ type: user, required: true }   # who booked",
            "{ type: user, required: false }   # who booked"
        )
    );
}

#[test]
fn set_adds_a_key_in_a_flow_map_and_a_field_like_its_siblings() {
    let out = ok(
        FILE,
        r#"[{"op":"set","path":"entities.Booking.fields.phone","value":{"type":"text","required":true}},
            {"op":"set","path":"entities.Booking.fields.notes.required","value":true}]"#,
    );
    assert_eq!(
        out,
        FILE.replace(
            "      notes:    { type: text }\n",
            "      notes:    { type: text, required: true }\n      phone: { type: text, required: true }\n"
        )
    );
}

#[test]
fn set_adds_an_entity_in_block_style_with_flow_leaves() {
    let out = ok(
        FILE,
        r#"[{"op":"set","path":"entities.Tag","value":{"fields":{"id":{"type":"uuid","primary":true},"label":{"type":"text"}},"private":true}}]"#,
    );
    let expected = FILE.replace(
        "\npolicies:",
        "  Tag:\n    fields:\n      id: { type: uuid, primary: true }\n      label: { type: text }\n    private: true\n\npolicies:",
    );
    assert_eq!(out, expected);
}

#[test]
fn set_replaces_a_block_with_a_scalar_and_the_other_way_round() {
    let out = ok(
        FILE,
        r#"[{"op":"set","path":"entities.Comment.fields","value":{"id":{"type":"uuid","primary":true}}}]"#,
    );
    assert!(
        out.contains(
            "  Comment:\n    fields:\n      id: { type: uuid, primary: true }\n\npolicies:"
        ),
        "{out}"
    );
    // A map where a scalar was, and a scalar where a map was.
    let out = ok(
        "svetra: \"0.1\"\nsystem: Shop\nentities:\n  A:\n    private: true\n    fields:\n      id: { type: uuid, primary: true }\n",
        r#"[{"op":"set","path":"entities.A.private","value":{"a":1,"b":[1,2]}},{"op":"set","path":"entities.A.fields.id","value":"uuid"}]"#,
    );
    assert_eq!(
        out,
        "svetra: \"0.1\"\nsystem: Shop\nentities:\n  A:\n    private:\n      a: 1\n      b: [1, 2]\n    fields:\n      id: uuid\n"
    );
}

#[test]
fn set_creates_the_maps_on_the_way() {
    let out = ok(
        "svetra: \"0.1\"\nsystem: Shop\nentities:\n  A:\n    fields:\n      id: { type: uuid, primary: true }\n",
        r#"[{"op":"set","path":"auth.password.min_length","value":12},
            {"op":"set","path":"holes.close","value":{"kind":"function","writes":["A"]}}]"#,
    );
    assert_eq!(
        out,
        "svetra: \"0.1\"\nsystem: Shop\nentities:\n  A:\n    fields:\n      id: { type: uuid, primary: true }\nauth:\n  password: { min_length: 12 }\nholes:\n  close: { kind: function, writes: [A] }\n"
    );
}

#[test]
fn set_fills_an_empty_map_and_a_null() {
    let out = ok(
        "holes: {}\nroles:\nauth: { sign_in: [email] }\n",
        r#"[{"op":"set","path":"holes.go","value":{"kind":"function"}},
            {"op":"set","path":"roles.admin","value":{"description":"Runs it"}},
            {"op":"set","path":"auth.invite_only","value":true}]"#,
    );
    assert_eq!(
        out,
        "holes:\n  go: { kind: function }\nroles:\n  admin: { description: \"Runs it\" }\nauth: { sign_in: [email], invite_only: true }\n"
    );
}

#[test]
fn strings_are_quoted_only_when_they_must_be() {
    let out = ok(
        "a: 1\n",
        r#"[{"op":"set","path":"b","value":"text"},{"op":"set","path":"c","value":"Account.id"},
            {"op":"set","path":"d","value":"owner == auth.user"},{"op":"set","path":"e","value":"true"},
            {"op":"set","path":"f","value":"0.1"},{"op":"set","path":"g","value":null},
            {"op":"set","path":"h","value":1.5},{"op":"set","path":"i","value":"say \"hi\""}]"#,
    );
    assert_eq!(
        out,
        "a: 1\nb: text\nc: Account.id\nd: \"owner == auth.user\"\ne: \"true\"\nf: \"0.1\"\ng: null\nh: 1.5\ni: \"say \\\"hi\\\"\"\n"
    );
}

#[test]
fn remove_takes_the_comment_above_and_the_lines_below() {
    let out = ok(FILE, r#"[{"op":"remove","path":"entities.Comment"}]"#);
    assert_eq!(
        out,
        FILE.replace(
            "  # Comments are removable.\n  Comment:\n    fields:\n      id:   { type: uuid, primary: true }\n      body: { type: text }\n",
            ""
        )
    );
    // A blank line ends the comment: the line before the policies' comment-free key stays.
    let out = ok(
        FILE,
        r#"[{"op":"remove","path":"entities.Booking.fields.customer"}]"#,
    );
    assert!(!out.contains("customer: {"), "{out}");
    assert!(out.contains(
        "      id:       { type: uuid, primary: true }\n      notes:    { type: text }\n"
    ));
}

#[test]
fn removing_the_last_entry_leaves_an_empty_map() {
    let out = ok(
        "entities:\n  A:\n    fields:\n      id: { type: uuid, primary: true }\npolicies:\n  p: { on: A, allow: [select], when: \"true\" }\n",
        r#"[{"op":"remove","path":"policies.p"}]"#,
    );
    assert_eq!(
        out,
        "entities:\n  A:\n    fields:\n      id: { type: uuid, primary: true }\npolicies: {}\n"
    );
}

#[test]
fn remove_in_a_flow_map_handles_first_middle_last_and_only() {
    let src = "m: { a: 1, b: 2, c: 3 }\nn: { a: 1 }\n";
    for (path, want) in [
        ("m.a", "m: { b: 2, c: 3 }\nn: { a: 1 }\n"),
        ("m.b", "m: { a: 1, c: 3 }\nn: { a: 1 }\n"),
        ("m.c", "m: { a: 1, b: 2 }\nn: { a: 1 }\n"),
        ("n.a", "m: { a: 1, b: 2, c: 3 }\nn: {}\n"),
    ] {
        assert_eq!(
            ok(src, &format!(r#"[{{"op":"remove","path":"{path}"}}]"#)),
            want,
            "{path}"
        );
    }
}

#[test]
fn rename_changes_the_key_only_and_declares_what_it_was() {
    let out = ok(
        FILE,
        r#"[{"op":"rename","path":"entities.Booking.fields.customer","to":"client"},
            {"op":"rename","path":"entities.Comment","to":"Remark"}]"#,
    );
    let expected = FILE
        .replace(
            "customer: { type: user, required: true }   # who booked",
            "client: { type: user, required: true, was: customer }   # who booked",
        )
        .replace(
            "  Comment:\n    fields:\n      id:   { type: uuid, primary: true }\n      body: { type: text }\n",
            "  Remark:\n    fields:\n      id:   { type: uuid, primary: true }\n      body: { type: text }\n    was: Comment\n",
        );
    assert_eq!(out, expected);
}

#[test]
fn rename_keeps_an_existing_was_and_adds_none_for_a_policy() {
    let out = ok(
        "entities:\n  B:\n    was: A\n    fields:\n      id: { type: uuid, primary: true }\npolicies:\n  p: { on: B, allow: [select], when: \"true\" }\n",
        r#"[{"op":"rename","path":"entities.B","to":"C"},{"op":"rename","path":"policies.p","to":"q"}]"#,
    );
    assert_eq!(
        out,
        "entities:\n  C:\n    was: A\n    fields:\n      id: { type: uuid, primary: true }\npolicies:\n  q: { on: B, allow: [select], when: \"true\" }\n"
    );
}

#[test]
fn a_rename_leaves_references_so_validation_reports_them() {
    let dir = scratch("rename-refs");
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, FILE).unwrap();
    let o = svetra(
        &dir,
        &["patch", "--json"],
        Some(r#"[{"op":"rename","path":"entities.Booking.fields.customer","to":"client"}]"#),
    );
    assert_eq!(o.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_str(&text(&o)).unwrap();
    assert_eq!(v["applied"], false);
    assert!(
        v["problems"][0]["code"]
            .as_str()
            .unwrap()
            .starts_with("SVT")
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        FILE,
        "wrote nothing"
    );
}

#[test]
fn edits_that_cannot_apply_are_all_listed_and_the_others_still_run() {
    let (out, problems) = edit(
        FILE,
        r#"[{"op":"remove","path":"entities.Nope"},
            {"op":"set","path":"system.deeper","value":1},
            {"op":"rename","path":"entities.Comment","to":"Booking"},
            {"op":"set","path":"entities.Booking.private","value":true}]"#,
    );
    let at: Vec<&str> = problems.iter().map(|p| p.at.as_str()).collect();
    assert_eq!(at, ["ops[0]", "ops[1]", "ops[2]"]);
    assert!(problems[0].message.contains("`entities.Nope` isn't there"));
    assert!(
        problems[1]
            .message
            .contains("`system` is a string, not a map")
    );
    assert!(problems[2].message.contains("already a `Booking`"));
    assert!(out.contains("    private: true\n") || out.contains("private: true"));
}

#[test]
fn crlf_and_a_missing_final_newline_are_kept() {
    let crlf = "a: 1\r\nb:\r\n  c: 2\r\n";
    assert_eq!(
        ok(crlf, r#"[{"op":"set","path":"b.d","value":3}]"#),
        "a: 1\r\nb:\r\n  c: 2\r\n  d: 3\r\n"
    );
    assert_eq!(ok("a: 1", r#"[{"op":"set","path":"a","value":2}]"#), "a: 2");
}

#[test]
fn a_comment_after_the_last_entry_stays_below_the_new_one() {
    let out = ok(
        "m:\n  a: 1\n  # trailing note\n\nz: 1\n",
        r#"[{"op":"set","path":"m.b","value":2}]"#,
    );
    assert_eq!(out, "m:\n  a: 1\n  b: 2\n  # trailing note\n\nz: 1\n");
}

#[test]
fn bad_edit_lists_are_usage_errors() {
    for (text, want) in [
        ("{}", "JSON array"),
        ("[1]", "ops[0] must be an object"),
        (r#"[{"op":"set","path":"a"}]"#, "needs a \"value\""),
        (r#"[{"op":"zap","path":"a"}]"#, "unknown op"),
        (r#"[{"op":"remove","path":"a..b"}]"#, "empty part"),
        (r#"[{"op":"remove","path":"a","value":1}]"#, "doesn't take"),
        (r#"[{"op":"rename","path":"a"}]"#, "needs a string \"to\""),
        (
            r#"[{"op":"set","path":"a","value":1,"extra":2}]"#,
            "unknown key",
        ),
    ] {
        let e = parse_ops(text).unwrap_err();
        assert!(e.contains(want), "{text}: {e}");
    }
}

// --- the command ------------------------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("svetra-patch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn svetra(dir: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_svetra"))
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(s) = stdin {
        child.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

const CHANGES: &str = r#"[
  {"op":"set","path":"entities.Booking.fields.phone","value":{"type":"text"}},
  {"op":"rename","path":"entities.Booking.fields.customer","to":"client"},
  {"op":"set","path":"policies.own_bookings.when","value":"client == auth.user"},
  {"op":"remove","path":"policies.all_comments"},
  {"op":"remove","path":"entities.Comment"}
]"#;

#[test]
fn a_patch_applies_and_says_what_it_changed_and_what_needs_a_yes() {
    let dir = scratch("apply");
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, FILE).unwrap();
    let o = svetra(&dir, &["patch", "--json"], Some(CHANGES));
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let v: serde_json::Value = serde_json::from_str(&text(&o)).unwrap();
    assert_eq!(v["applied"], true);
    assert_eq!(v["dry_run"], false);
    assert_eq!(v["problems"].as_array().unwrap().len(), 0);
    let kinds: Vec<&str> = v["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"remove_entity") && kinds.contains(&"rename_field"),
        "{kinds:?}"
    );
    assert_eq!(v["needs_yes"], serde_json::json!(["remove_entity"]));
    let now = std::fs::read_to_string(&file).unwrap();
    assert!(now.contains("client: { type: user, required: true, was: customer }   # who booked"));
    assert!(now.starts_with("svetra: \"0.1\"\nsystem: Shop\n\n# What the shop keeps.\n"));
    assert_eq!(v["revision"]["now"], svetra::compile::revision(&now));
    // Notes are what the edits brought, and nothing the file already said.
    assert!(
        v["notes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|n| n["code"] != "SVT009"),
        "{v}"
    );
}

#[test]
fn the_patch_then_plan_shows_a_rename_and_not_a_drop() {
    let dir = scratch("roundtrip");
    let file = dir.join("svetra.yaml");
    let old = dir.join("old.yaml");
    std::fs::write(&file, FILE).unwrap();
    std::fs::write(&old, FILE).unwrap();
    let ops = r#"[{"op":"rename","path":"entities.Booking.fields.customer","to":"client"},
                  {"op":"set","path":"policies.own_bookings.when","value":"client == auth.user"}]"#;
    assert_eq!(svetra(&dir, &["patch"], Some(ops)).status.code(), Some(0));
    let o = svetra(
        &dir,
        &["plan", "--from", "old.yaml", "--json", "--no-write"],
        None,
    );
    let plan = text(&o);
    assert!(plan.contains("rename_field"), "{plan}");
    assert!(!plan.contains("remove_field"), "{plan}");
}

#[test]
fn dry_run_writes_nothing() {
    let dir = scratch("dry");
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, FILE).unwrap();
    let o = svetra(&dir, &["patch", "--dry-run", "--json"], Some(CHANGES));
    assert_eq!(o.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_str(&text(&o)).unwrap();
    assert_eq!(
        (v["applied"].clone(), v["dry_run"].clone()),
        (true.into(), true.into())
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
}

#[test]
fn an_invalid_result_writes_nothing_and_reports_the_position_in_the_new_file() {
    let dir = scratch("invalid");
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, FILE).unwrap();
    let ops = r#"[{"op":"set","path":"entities.Booking.fields.phone","value":{"type":"text"}},
                  {"op":"set","path":"entities.Booking.fields.bad","value":{"type":"nonsense"}}]"#;
    let o = svetra(&dir, &["patch", "--json"], Some(ops));
    assert_eq!(o.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_str(&text(&o)).unwrap();
    assert_eq!(v["applied"], false);
    let p = &v["problems"][0];
    assert_eq!(p["code"], "SVT023");
    assert!(
        p["at"].as_str().unwrap().ends_with("svetra.yaml:12:20"),
        "{p}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
}

#[test]
fn one_bad_edit_among_three_leaves_the_file_byte_identical() {
    let dir = scratch("atomic");
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, FILE).unwrap();
    let ops = r#"[{"op":"set","path":"entities.Booking.fields.phone","value":{"type":"text"}},
                  {"op":"remove","path":"entities.Nope"},
                  {"op":"remove","path":"policies.all_comments"}]"#;
    let o = svetra(&dir, &["patch"], Some(ops));
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o).contains("ops[1] edit"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
}

#[test]
fn a_stale_revision_is_a_conflict_that_writes_nothing() {
    let dir = scratch("conflict");
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, FILE).unwrap();
    let ops = r#"[{"op":"set","path":"entities.Booking.fields.phone","value":{"type":"text"}}]"#;
    let o = svetra(
        &dir,
        &["patch", "--expected-revision", "000000000000", "--json"],
        Some(ops),
    );
    assert_eq!(o.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_str(&text(&o)).unwrap();
    assert_eq!(v["conflict"], true);
    assert_eq!(v["revision"]["expected"], "000000000000");
    assert_eq!(v["revision"]["now"], svetra::compile::revision(FILE));
    assert!(
        v["hint"]
            .as_str()
            .unwrap()
            .contains("changed since you read it")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
    // The right revision applies.
    let rev = svetra::compile::revision(FILE);
    let o = svetra(&dir, &["patch", "--expected-revision", &rev], Some(ops));
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
}

#[test]
fn context_prints_the_revision_the_patch_takes() {
    let dir = scratch("context");
    std::fs::write(dir.join("svetra.yaml"), FILE).unwrap();
    let o = svetra(&dir, &["context"], None);
    let first = text(&o).lines().next().unwrap().to_string();
    assert_eq!(
        first,
        format!(
            "Shop (svetra 0.1), revision {}",
            svetra::compile::revision(FILE)
        )
    );
}

#[test]
fn patch_reads_ops_from_a_file_and_usage_errors_exit_two() {
    let dir = scratch("usage");
    std::fs::write(dir.join("svetra.yaml"), FILE).unwrap();
    std::fs::write(
        dir.join("ops.json"),
        r#"[{"op":"set","path":"entities.Booking.fields.phone","value":{"type":"text"}}]"#,
    )
    .unwrap();
    let o = svetra(&dir, &["patch", "--ops", "ops.json"], None);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    for (args, input) in [
        (vec!["patch"], Some("not json")),
        (vec!["patch", "--ops", "missing.json"], None),
        (vec!["patch", "--nope"], Some("[]")),
        (vec!["patch", "gone.yaml"], Some("[]")),
    ] {
        let o = svetra(&dir, &args, input);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn no_change_is_reported_as_no_change() {
    let dir = scratch("nochange");
    let file = dir.join("svetra.yaml");
    std::fs::write(&file, FILE).unwrap();
    let o = svetra(
        &dir,
        &["patch"],
        Some(r#"[{"op":"set","path":"system","value":"Shop"}]"#),
    );
    assert_eq!(o.status.code(), Some(0));
    assert!(text(&o).contains("no change"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
}

#[test]
fn a_patch_to_a_file_that_was_invalid_still_applies_when_the_result_is_valid() {
    let dir = scratch("was-invalid");
    let file = dir.join("svetra.yaml");
    let broken = FILE.replace("system: Shop\n", "system: shop\n");
    std::fs::write(&file, &broken).unwrap();
    let o = svetra(
        &dir,
        &["patch", "--json"],
        Some(r#"[{"op":"set","path":"system","value":"Shop"}]"#),
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let v: serde_json::Value = serde_json::from_str(&text(&o)).unwrap();
    assert_eq!(v["applied"], true);
    assert!(
        v["comparison"]
            .as_str()
            .unwrap()
            .contains("doesn't validate")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), FILE);
}

#[test]
fn a_new_map_follows_its_siblings_and_words_that_yaml_reads_as_booleans_are_quoted() {
    // Siblings in block style: the new map is block too. `y` is quoted: some YAML tools read it as true.
    let out = ok(
        "e:\n  m:\n    a: 1\n",
        r#"[{"op":"set","path":"e.p","value":{"y":1}}]"#,
    );
    assert_eq!(out, "e:\n  m:\n    a: 1\n  p:\n    \"y\": 1\n");
}

// --- every fixture, every path -------------------------------------------------------------------

const FIXTURES: &[(&str, &str)] = &[
    ("finledger", include_str!("fixtures/finledger.yaml")),
    ("bookings", include_str!("fixtures/bookings.yaml")),
    ("clinic", include_str!("fixtures/clinic.yaml")),
    ("approvals", include_str!("fixtures/approvals.yaml")),
    ("forum", include_str!("fixtures/forum.yaml")),
    ("posts", include_str!("fixtures/posts.yaml")),
    ("profiles", include_str!("fixtures/profiles.yaml")),
    ("teams", include_str!("fixtures/teams.yaml")),
    ("audited", include_str!("fixtures/audited.yaml")),
];

fn to_json(n: &svetra::yaml::Node) -> serde_json::Value {
    use svetra::yaml::Value;
    match &n.value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => (*b).into(),
        Value::Int(i) => (*i).into(),
        Value::Float(f) => (*f).into(),
        Value::Str(s) => s.clone().into(),
        Value::Seq(items) => items.iter().map(to_json).collect(),
        Value::Map(m) => m
            .iter()
            .map(|(k, v)| (k.as_str().unwrap_or("").to_string(), to_json(v)))
            .collect::<serde_json::Map<_, _>>()
            .into(),
    }
}

/// Every path through maps, with its value: `(["entities", "A"], ...)`.
fn paths(
    n: &svetra::yaml::Node,
    at: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, serde_json::Value)>,
) {
    if let svetra::yaml::Value::Map(m) = &n.value {
        for (k, v) in m {
            at.push(k.as_str().unwrap_or("").to_string());
            out.push((at.clone(), to_json(v)));
            paths(v, at, out);
            at.pop();
        }
    }
}

fn all_paths(src: &str) -> Vec<(Vec<String>, serde_json::Value)> {
    let root = svetra::yaml::parse(src).unwrap();
    let mut out = Vec::new();
    paths(&root, &mut Vec::new(), &mut out);
    out
}

fn ops_json(op: &str, path: &[String], value: Option<&serde_json::Value>) -> String {
    let mut o = serde_json::json!({ "op": op, "path": path.join(".") });
    if let Some(v) = value {
        o["value"] = v.clone();
    }
    serde_json::json!([o]).to_string()
}

/// Setting a path to the value it already has changes nothing the compiler can see.
#[test]
fn setting_every_path_to_its_own_value_changes_nothing() {
    let mut count = 0;
    for (name, src) in FIXTURES {
        let before = svetra::understand::context::context(&svetra::load::load(src).0);
        for (path, value) in all_paths(src) {
            let (out, problems) = edit(src, &ops_json("set", &path, Some(&value)));
            assert!(problems.is_empty(), "{name} {path:?}: {problems:#?}");
            let after = svetra::understand::context::context(&svetra::load::load(&out).0);
            assert_eq!(
                before, after,
                "{name}: set {path:?} to itself changed the design:\n{out}"
            );
            count += 1;
        }
    }
    assert!(count > 500, "only {count} paths");
}

/// Removing any path leaves text that still parses, with that path gone and every other path
/// still there with its value.
#[test]
fn removing_every_path_removes_exactly_that_path() {
    for (name, src) in FIXTURES {
        let all = all_paths(src);
        for (path, _) in &all {
            let (out, problems) = edit(src, &ops_json("remove", path, None));
            assert!(problems.is_empty(), "{name} {path:?}: {problems:#?}");
            let Ok(root) = svetra::yaml::parse(&out) else {
                panic!("{name}: removing {path:?} left unparseable text:\n{out}");
            };
            let mut left = Vec::new();
            paths(&root, &mut Vec::new(), &mut left);
            // Everything that isn't the path, below it, or above it (whose value holds it) is as it was.
            for (p, v) in all
                .iter()
                .filter(|(p, _)| !p.starts_with(path) && !path.starts_with(p))
            {
                let now = left.iter().find(|(q, _)| q == p).map(|(_, v)| v);
                assert_eq!(
                    now,
                    Some(v),
                    "{name}: removing {path:?} changed {p:?}:\n{out}"
                );
            }
            assert!(
                !left.iter().any(|(p, _)| p.starts_with(path)),
                "{name}: {path:?} is still there:\n{out}"
            );
        }
    }
}

/// A new key under every map lands as the last child, readable back with its value.
#[test]
fn adding_a_key_under_every_map_reads_back() {
    for (name, src) in FIXTURES {
        for (path, value) in all_paths(src) {
            if !value.is_object() {
                continue;
            }
            let mut deeper = path.clone();
            deeper.push("zz_new".into());
            let new = serde_json::json!({ "type": "text", "list": ["a", "b"] });
            let (out, problems) = edit(src, &ops_json("set", &deeper, Some(&new)));
            assert!(problems.is_empty(), "{name} {deeper:?}: {problems:#?}");
            let Ok(root) = svetra::yaml::parse(&out) else {
                panic!("{name}: adding {deeper:?} left unparseable text:\n{out}");
            };
            let mut found = Vec::new();
            paths(&root, &mut Vec::new(), &mut found);
            let got = found.iter().find(|(p, _)| p == &deeper).map(|(_, v)| v);
            assert_eq!(
                got,
                Some(&new),
                "{name}: {deeper:?} reads back wrong:\n{out}"
            );
        }
    }
}

/// Renaming any key keeps its value (and an entity or a field gains only `was`).
#[test]
fn renaming_every_path_keeps_its_value() {
    for (name, src) in FIXTURES {
        for (path, value) in all_paths(src) {
            let ops =
                serde_json::json!([{ "op": "rename", "path": path.join("."), "to": "zz_renamed" }]);
            let (out, problems) = edit(src, &ops.to_string());
            assert!(problems.is_empty(), "{name} {path:?}: {problems:#?}");
            let Ok(root) = svetra::yaml::parse(&out) else {
                panic!("{name}: renaming {path:?} left unparseable text:\n{out}");
            };
            let mut found = Vec::new();
            paths(&root, &mut Vec::new(), &mut found);
            let mut renamed = path.clone();
            if let Some(last) = renamed.last_mut() {
                *last = "zz_renamed".into();
            }
            let got = found
                .iter()
                .find(|(p, _)| p == &renamed)
                .map(|(_, v)| v.clone());
            let mut want = value.clone();
            let declares = matches!(path.as_slice(), [a, _] if a == "entities")
                || matches!(path.as_slice(), [a, _, c, _] if a == "entities" && c == "fields");
            if declares && let Some(m) = want.as_object_mut() {
                m.entry("was")
                    .or_insert(path.last().cloned().unwrap_or_default().into());
            }
            assert_eq!(got, Some(want), "{name}: renaming {path:?}:\n{out}");
        }
    }
}
