//! Webhook holes (docs/format.md, Webhook holes): loading, validation and diagnostics.

use svetra::load::load;
use svetra::model::{HoleKind, Scheme};
use svetra::report::validate;

/// An app with an invoice and a payment, and the holes given, each as the YAML under `holes:`.
fn doc(holes: &str) -> String {
    format!(
        "svetra: \"0.1\"\nsystem: App\nauth:\n  sign_in: [email]\nroles:\n  member: {{}}\nentities:\n  Invoice:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user, required: true }}\n      amount: {{ type: decimal, precision: 12, scale: 2, required: true }}\n      paid: {{ type: bool, default: false }}\n  Payment:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      invoice: {{ type: ref, entity: Invoice, required: true }}\npolicies:\n  own:\n    on: Invoice\n    allow: [select]\n    when: \"owner == auth.user\"\n  pay:\n    on: Payment\n    allow: [select]\n    when: \"invoice.owner == auth.user\"\nholes:\n{holes}"
    )
}

fn found(holes: &str) -> Vec<(String, String)> {
    validate(&doc(holes))
        .into_iter()
        .filter(|d| !matches!(d.code.id(), "SVT009" | "SVT010"))
        .map(|d| (d.code.id().to_string(), d.message))
        .collect()
}

fn codes(holes: &str) -> Vec<String> {
    found(holes).into_iter().map(|(c, _)| c).collect()
}

const GOOD: &str = "  paid:\n    kind: webhook\n    secret: PAYMENTS_WEBHOOK_SECRET\n    input:\n      invoice: Invoice.id\n      amount: Invoice.amount\n      event_id: text\n    dedupe: event_id\n    reads: [Invoice]\n    writes: [Invoice, Payment]\n";

#[test]
fn a_valid_webhook_has_no_diagnostics_and_the_defaults_apply() {
    assert_eq!(found(GOOD), vec![]);
    let (d, _) = load(&doc(GOOD));
    let HoleKind::Webhook(w) = &d.holes[0].kind else {
        panic!("a webhook");
    };
    assert_eq!(w.secret, "PAYMENTS_WEBHOOK_SECRET");
    assert_eq!(
        (w.scheme, w.header.as_str()),
        (Scheme::HmacSha256, "x-signature")
    );
    assert_eq!(w.dedupe.as_ref().map(|d| d.name.as_str()), Some("event_id"));
}

#[test]
fn a_signature_can_be_a_shared_secret_in_another_header() {
    let hole =
        format!("{GOOD}    signature:\n      scheme: shared_secret\n      header: X-Hook-Token\n");
    assert_eq!(found(&hole), vec![]);
    let (d, _) = load(&doc(&hole));
    let HoleKind::Webhook(w) = &d.holes[0].kind else {
        panic!("a webhook");
    };
    // Header names are case-insensitive, so they are kept in lower case.
    assert_eq!(
        (w.scheme, w.header.as_str()),
        (Scheme::SharedSecret, "x-hook-token")
    );
}

#[test]
fn svt028_a_webhook_needs_a_secret_and_inputs() {
    for missing in ["    secret: PAYMENTS_WEBHOOK_SECRET\n"] {
        assert_eq!(
            codes(&GOOD.replace(missing, "")),
            ["SVT028"],
            "without {missing:?}"
        );
    }
    let no_input = "  paid:\n    kind: webhook\n    secret: S\n";
    assert_eq!(codes(no_input), ["SVT028"]);
}

#[test]
fn svt039_a_pasted_key_is_refused_and_never_echoed() {
    for pasted in [
        "whsec_abc123",
        "sk_live_51Hx",
        "my secret",
        "9LIVES",
        "lower_case",
        "",
    ] {
        let hole = GOOD.replace("PAYMENTS_WEBHOOK_SECRET", &format!("\"{pasted}\""));
        let all = found(&hole);
        assert_eq!(
            all.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>(),
            ["SVT039"],
            "{pasted:?}"
        );
        if !pasted.is_empty() {
            assert!(
                !all[0].1.contains(pasted),
                "the message printed the value: {}",
                all[0].1
            );
        }
    }
}

#[test]
fn svt029_a_scheme_and_a_header_must_be_known_and_well_formed() {
    let bad_scheme = format!("{GOOD}    signature:\n      scheme: md5\n");
    assert_eq!(codes(&bad_scheme), ["SVT029"]);
    let bad_header = format!("{GOOD}    signature:\n      header: \"x signature\"\n");
    assert_eq!(codes(&bad_header), ["SVT029"]);
}

#[test]
fn svt020_keys_a_webhook_does_not_take() {
    for extra in [
        "    as: anyone\n",
        "    output: Payment\n",
        "    schedule: \"0 3 * * *\"\n",
    ] {
        assert_eq!(codes(&format!("{GOOD}{extra}")), ["SVT020"], "{extra:?}");
    }
    // And the other kinds don't take the webhook's keys.
    let function = "  f:\n    kind: function\n    secret: SOME_SECRET\n    writes: [Payment]\n";
    assert_eq!(codes(function), ["SVT020"]);
}

#[test]
fn svt002_dedupe_must_name_an_input() {
    let hole = GOOD.replace("dedupe: event_id", "dedupe: nothing");
    let all = found(&hole);
    assert_eq!(
        all.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>(),
        ["SVT002"]
    );
    assert!(all[0].1.contains("isn't an input"));
}

#[test]
fn when_works_on_the_inputs_but_nobody_is_signed_in() {
    let ok = format!("{GOOD}    when: \"amount > 0\"\n");
    assert_eq!(found(&ok), vec![]);
    for cond in ["invoice.owner == auth.user", "has_role('member')"] {
        let hole = format!("{GOOD}    when: \"{cond}\"\n");
        assert_eq!(codes(&hole), ["SVT003"], "{cond}");
    }
    // One hop through an input is fine, as for a function, when the hole may read what it reaches.
    let hop = format!("{GOOD}    when: \"invoice.paid == false\"\n");
    assert_eq!(found(&hop), vec![]);
}

#[test]
fn a_webhook_may_require_and_audit_and_grant_like_a_function() {
    let hole = format!("{GOOD}    require:\n      - \"amount > 0\"\n    audit: true\n");
    assert_eq!(found(&hole), vec![]);
    // grants on a webhook is not the SVT040 warning: a schedule or a trigger has nobody asking, a webhook has the sender.
    let granting = format!("{GOOD}    grants: [member]\n");
    assert_eq!(found(&granting), vec![]);
}

#[test]
fn a_webhook_with_one_input_and_no_dedupe_is_valid() {
    let hole = "  ping:\n    kind: webhook\n    secret: PING_SECRET\n    input:\n      n: int\n    reads: [Invoice]\n    writes: [Payment, Invoice]\n";
    assert_eq!(found(hole), vec![]);
}
