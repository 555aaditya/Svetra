use svetra::diag::Code;
use svetra::load::load;
use svetra::model::*;

fn codes(src: &str) -> Vec<&'static str> {
    load(src).1.iter().map(|d| d.code.id()).collect()
}

const HEAD: &str = "svetra: \"0.1\"\nsystem: App\n";

#[test]
fn spec_example_loads_cleanly() {
    let (doc, diags) = load(include_str!("fixtures/finledger.yaml"));
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(doc.system.name, "FinLedger");
    assert_eq!(doc.entities.len(), 2);
    let account = &doc.entities[0];
    let balance = account.fields.iter().find(|f| f.name == "balance").unwrap();
    assert_eq!(
        balance.ty,
        FieldType::Money {
            precision: 18,
            scale: 6
        }
    );
    assert!(
        account
            .fields
            .iter()
            .find(|f| f.name == "id")
            .unwrap()
            .required,
        "primary implies required"
    );
    assert_eq!(account.constraints[0].expr, "balance >= 0");
    assert_eq!(doc.policies[1].allow, vec![Action::Select]);
    let transfer = &doc.holes[0];
    assert_eq!(transfer.writes.len(), 2);
    assert!(
        matches!(&transfer.input[2].ty, ParamType::FieldOf { entity, field } if entity.name == "Transaction" && field == "amount")
    );
    assert!(matches!(&transfer.output, Some(ParamType::Entity(e)) if e.name == "Transaction"));
}

#[test]
fn syntax_error_is_svt019_with_position() {
    let (_, diags) = load("svetra: \"0.1\"\nentities: [\n");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, Code::Svt019);
    assert!(diags[0].line >= 2);
}

#[test]
fn unknown_key_is_svt020_at_the_key() {
    let (_, diags) = load(&format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true, primay: true }}\n"
    ));
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert_eq!(diags[0].code, Code::Svt020);
    assert_eq!(diags[0].path, "entities.A.fields.id.primay");
    assert_eq!((diags[0].line, diags[0].col), (6, 39));
}

#[test]
fn unquoted_version_is_svt025() {
    assert_eq!(
        codes("svetra: 0.1\nsystem: App\nentities: {}\n"),
        ["SVT025"]
    );
}

#[test]
fn missing_keys_are_svt028() {
    assert_eq!(codes("entities: {}\n"), ["SVT028", "SVT028"]);
    assert_eq!(
        codes(&format!(
            "{HEAD}entities:\n  A:\n    fields:\n      id: {{ primary: true }}\n"
        )),
        ["SVT028"]
    );
}

#[test]
fn wrong_kinds_are_svt029() {
    assert_eq!(codes(&format!("{HEAD}entities: [A]\n")), ["SVT029"]);
    assert_eq!(
        codes(&format!(
            "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, required: yes }}\n"
        )),
        ["SVT029"]
    );
}

#[test]
fn type_problems_are_svt023() {
    let field = |spec: &str| {
        codes(&format!(
            "{HEAD}entities:\n  A:\n    fields:\n      x: {spec}\n"
        ))
    };
    assert_eq!(field("{ type: string }"), ["SVT023"]);
    assert_eq!(
        field("{ type: money, precision: 40, scale: 2 }"),
        ["SVT023"]
    );
    assert_eq!(field("{ type: money, precision: 10 }"), ["SVT028"]);
    assert_eq!(field("{ type: enum, values: [a, a] }"), ["SVT023"]);
    assert_eq!(field("{ type: text, values: [a] }"), ["SVT023"]);
    assert_eq!(field("{ type: text, auto: now }"), ["SVT023"]);
    assert_eq!(
        field("{ type: enum, values: [a, b], default: c }"),
        ["SVT023"]
    );
    assert_eq!(field("{ type: user, default: x }"), ["SVT023"]);
    assert!(field("{ type: enum, values: [a, b], default: a }").is_empty());
}

#[test]
fn holes_and_policies_report_their_own_problems() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n\
         policies:\n  p:\n    on: A\n    allow: [select, read]\n    when: \"true\"\n    as: everyone\n\
         holes:\n  h:\n    kind: webhook\n  g:\n    kind: function\n    input:\n      n: money\n"
    );
    assert_eq!(codes(&src), ["SVT029", "SVT029", "SVT023", "SVT023"]);
}

#[test]
fn collects_every_problem_instead_of_stopping() {
    let src = format!(
        "{HEAD}entities:\n  A:\n    fields:\n      a: {{ type: nope }}\n      b: {{ type: nope }}\n    extra: 1\n"
    );
    assert_eq!(codes(&src), ["SVT020", "SVT023", "SVT023"]);
}
