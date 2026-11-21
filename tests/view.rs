use serde_json::Value;
use svetra::compile::access_tests::tests;
use svetra::load::load;
use svetra::understand::view::{json, model, page};

const EXAMPLES: [(&str, &str); 6] = [
    ("finledger", include_str!("fixtures/finledger.yaml")),
    ("posts", include_str!("fixtures/posts.yaml")),
    ("bookings", include_str!("fixtures/bookings.yaml")),
    ("clinic", include_str!("fixtures/clinic.yaml")),
    ("teams", include_str!("fixtures/teams.yaml")),
    ("approvals", include_str!("fixtures/approvals.yaml")),
];

fn doc(src: &str) -> svetra::model::Document {
    let (doc, diags) = load(src);
    assert!(diags.is_empty(), "{diags:#?}");
    doc
}

#[test]
fn view_models_match_golden_files() {
    for (name, src) in EXAMPLES {
        let want = std::fs::read_to_string(format!("tests/golden/{name}.view.json")).unwrap();
        assert_eq!(json(&doc(src), src), want, "{name}");
    }
}

/// Each cell's rows and test numbers point at checks in the generated file for the same
/// person, action, entity and row, with the same expected result.
#[test]
fn the_page_and_the_access_tests_agree() {
    for (name, src) in EXAMPLES {
        let d = doc(src);
        let text = tests(&d);
        // pgTAP numbers every statement after `plan`, from 1.
        let numbered: Vec<&str> = text
            .lines()
            .skip_while(|l| !l.starts_with("select plan("))
            .skip(1)
            .filter(|l| {
                l.starts_with("select lives_ok(")
                    || l.starts_with("select is(")
                    || l.starts_with("select skip(")
            })
            .collect();
        let m = model(&d, src);
        let mut seen = 0;
        for c in m["cells"].as_array().unwrap() {
            let (person, entity, action) = (
                c["person"].as_str().unwrap(),
                c["entity"].as_str().unwrap(),
                c["action"].as_str().unwrap(),
            );
            let rows = c["rows"].as_array().unwrap();
            let tests = c["tests"].as_array().unwrap();
            assert_eq!(rows.len(), tests.len(), "{name} {person} {action} {entity}");
            for (r, n) in rows.iter().zip(tests) {
                let line = numbered[n.as_u64().unwrap() as usize - 1];
                let row = r["row"].as_str().unwrap();
                let can = match r["allowed"] {
                    Value::Bool(true) => "can",
                    Value::Bool(false) => "can't",
                    _ => {
                        assert!(line.starts_with("select skip("), "{name}: {line}");
                        continue;
                    }
                };
                let words = format!(
                    "'{}'",
                    format!("{person} {can} {action} {entity} {row}").replace('\'', "''")
                );
                assert!(
                    line.contains(&words),
                    "{name}: test {n} is `{line}`, want {words}"
                );
                seen += 1;
            }
        }
        assert!(seen > 0, "{name}");
    }
}

/// A reviewer's update may change only the status, and the cell says so in `explain`'s words.
#[test]
fn a_change_cell_names_the_fields_a_column_rule_allows() {
    let src = include_str!("fixtures/approvals.yaml");
    let m = model(&doc(src), src);
    let cells = m["cells"].as_array().unwrap();
    let change = |person: &str| {
        cells
            .iter()
            .find(|c| c["person"] == person && c["entity"] == "Doc" && c["action"] == "change")
            .unwrap()
    };
    let only = change("bob")["only"].as_array().unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(only[0]["fields"], serde_json::json!(["status"]));
    assert_eq!(only[0]["sentence"], "They can change only its status.");
    assert!(
        only[0]["rule"]
            .as_str()
            .unwrap()
            .contains("its reviewer is them")
    );
    // Not signed in: the reviewers' rule doesn't cover them. Other actions carry no `only`.
    assert!(change("anon").get("only").is_none());
    assert!(
        cells
            .iter()
            .all(|c| c["action"] == "change" || c.get("only").is_none())
    );
    // Entities without a column rule never get the key.
    for (name, src) in EXAMPLES.iter().filter(|(n, _)| *n != "approvals") {
        let m = model(&doc(src), src);
        assert!(
            m["cells"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c.get("only").is_none()),
            "{name}"
        );
    }
}

#[test]
fn the_page_is_deterministic_and_self_contained() {
    for (name, src) in EXAMPLES {
        let d = doc(src);
        let a = page(&d, src);
        assert_eq!(a, page(&d, src), "{name}");
        assert!(!a.contains("{{"), "{name}: a placeholder was left in");
        assert!(a.contains("default-src 'none'"), "{name}: no CSP");
        for net in ["http://", "https://"] {
            let outside = a.matches(net).count();
            // Only the SVG namespace URI, which is an identifier, not a request.
            assert_eq!(
                outside,
                if net == "http://" { 1 } else { 0 },
                "{name}: {net}"
            );
        }
        // The embedded model can't end its script element early.
        let data = a
            .split("id=\"model\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        assert!(!data.contains("</"), "{name}");
    }
}

#[test]
fn the_page_never_uses_jargon() {
    for (name, src) in EXAMPLES {
        let m = model(&doc(src), src);
        let mut words = String::new();
        let mut collect = |v: &Value| words.push_str(&v.to_string().to_lowercase());
        for key in ["people", "entities", "signing_in", "before_going_live"] {
            collect(&m[key]);
        }
        for c in m["cells"].as_array().unwrap() {
            collect(&c["because"]);
            collect(&c["rules"]);
            collect(&c["only"]);
            for r in c["rows"].as_array().unwrap() {
                collect(&r["label"]);
            }
        }
        for h in m["holes"].as_array().unwrap() {
            collect(&h["lines"]);
        }
        for word in [
            "rls",
            "policy",
            "foreign key",
            "schema",
            "migration",
            "uuid",
            "constraint",
            "row-level",
        ] {
            assert!(!words.contains(word), "{name} page says `{word}`");
        }
    }
}
