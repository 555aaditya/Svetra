//! The JSON Schema is hand-written for editors. These tests fail when it drifts from the loader.

use serde_json::Value;
use std::collections::BTreeSet;
use svetra::load::{self, load};

fn schema() -> Value {
    serde_json::from_str(include_str!("../schema/svetra.schema.json"))
        .expect("schema is valid JSON")
}

fn props(v: &Value) -> BTreeSet<String> {
    v["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .cloned()
        .collect()
}

fn set(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|k| k.to_string()).collect()
}

#[test]
fn schema_keys_match_the_loader() {
    let s = schema();
    let defs = &s["$defs"];
    assert_eq!(props(&s), set(load::TOP_KEYS), "top level");
    assert_eq!(props(&defs["entity"]), set(load::ENTITY_KEYS), "entity");
    assert_eq!(props(&defs["field"]), set(load::FIELD_KEYS), "field");
    assert_eq!(props(&defs["policy"]), set(load::POLICY_KEYS), "policy");
    assert_eq!(props(&defs["hole"]), set(load::HOLE_KEYS), "hole");
    assert_eq!(
        props(&defs["paramSpec"]),
        set(load::PARAM_SPEC_KEYS),
        "param spec"
    );
    assert_eq!(props(&defs["role"]), set(load::ROLE_KEYS), "role");
    assert_eq!(props(&defs["auth"]), set(load::AUTH_KEYS), "auth");
    assert_eq!(
        props(&defs["password"]),
        set(load::PASSWORD_KEYS),
        "password"
    );
}

#[test]
fn schema_types_match_the_loader() {
    let s = schema();
    let in_schema: BTreeSet<String> = s["$defs"]["field"]["properties"]["type"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_string())
        .collect();
    assert_eq!(in_schema, set(load::TYPE_NAMES));
    assert_eq!(s["properties"]["svetra"]["const"], load::FORMAT_VERSION);
    for t in load::TYPE_NAMES {
        let opts = match *t {
            "decimal" | "money" => ", precision: 10, scale: 2",
            "enum" => ", values: [a]",
            "ref" => ", entity: A",
            _ => "",
        };
        let src = format!(
            "svetra: \"0.1\"\nsystem: App\nentities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      x: {{ type: {t}{opts} }}\n"
        );
        let (_, diags) = load(&src);
        assert!(
            diags.is_empty(),
            "loader rejects schema type `{t}`: {diags:?}"
        );
    }
}
