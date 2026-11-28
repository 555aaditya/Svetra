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
    let hole = GOOD.replace("    secret: PAYMENTS_WEBHOOK_SECRET\n", "");
    assert_eq!(codes(&hole), ["SVT028"]);
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

fn compiled(src: &str) -> Vec<svetra::compile::Output> {
    svetra::compile::compile(src, "svetra.yaml")
        .expect("valid")
        .0
}

fn output<'a>(all: &'a [svetra::compile::Output], path: &str) -> &'a str {
    &all.iter().find(|o| o.path == path).expect(path).contents
}

/// Checked on Postgres 16 by `scripts/pg-check.sh` (`tests/pg/webhooks_assert.sql`).
#[test]
fn webhooks_sql_matches_golden_file() {
    let all = compiled(include_str!("fixtures/webhooks.yaml"));
    let sql = output(&all, "supabase/schemas/svetra.sql");
    assert_eq!(
        sql.split_once("\n\n").expect("header").1,
        include_str!("golden/webhooks.sql")
    );
}

/// Run on a real local Supabase by `scripts/supabase-check.sh`: signed calls, bad signatures, wrong
/// shapes, the guard and a repeat delivery.
#[test]
fn edge_functions_match_golden_files() {
    let all = compiled(include_str!("fixtures/webhooks.yaml"));
    for (name, golden) in [
        (
            "payment_received",
            include_str!("golden/webhooks.payment_received.ts"),
        ),
        (
            "form_submitted",
            include_str!("golden/webhooks.form_submitted.ts"),
        ),
    ] {
        let ts = output(&all, &format!("supabase/functions/{name}/index.ts"));
        assert_eq!(ts.split_once('\n').expect("header").1, golden, "{name}");
    }
}

#[test]
fn only_webhooks_get_an_edge_function_and_the_client_has_no_call_for_them() {
    let all = compiled(include_str!("fixtures/webhooks.yaml"));
    let paths: Vec<&str> = all.iter().map(|o| o.path.as_str()).collect();
    assert!(paths.contains(&"supabase/functions/payment_received/index.ts"));
    assert!(paths.contains(&"supabase/functions/form_submitted/index.ts"));
    let ts = output(&all, "src/svetra.gen.ts");
    assert!(!ts.contains("payment_received") && !ts.contains("form_submitted"));
    for src in [
        include_str!("fixtures/finledger.yaml"),
        include_str!("fixtures/triggers.yaml"),
        include_str!("fixtures/jobs.yaml"),
    ] {
        assert!(compiled(src).iter().all(|o| !o.path.contains("functions/")));
    }
}

#[test]
fn the_function_never_holds_a_secret_and_names_only_the_secret_and_the_header() {
    let all = compiled(include_str!("fixtures/webhooks.yaml"));
    let ts = output(&all, "supabase/functions/payment_received/index.ts");
    assert!(ts.contains("const SECRET = \"PAYMENTS_WEBHOOK_SECRET\";"));
    assert!(ts.contains("const HEADER = \"x-signature\";"));
    // The proof is read from the environment, and the body is read raw before it is parsed.
    assert!(ts.contains("Deno.env.get(SECRET)"));
    assert!(ts.find("arrayBuffer()").unwrap() < ts.find("JSON.parse").unwrap());
    // No library is imported, so a call never waits on the network to start.
    assert!(!ts.contains("import "));
    // The shared-secret function compares, and does not compute a signature.
    let shared = output(&all, "supabase/functions/form_submitted/index.ts");
    assert!(shared.contains("same(bytes(secret), bytes(given))"));
    assert!(!shared.contains("crypto.subtle"));
    assert!(shared.contains("const HEADER = \"x-form-token\";"));
}

#[test]
fn the_public_function_is_for_the_service_role_alone() {
    let all = compiled(include_str!("fixtures/webhooks.yaml"));
    let sql = output(&all, "supabase/schemas/svetra.sql");
    for name in ["payment_received", "form_submitted"] {
        assert!(sql.contains(&format!("\"public\".\"{name}\"(")), "{name}");
    }
    assert_eq!(sql.matches("to service_role;").count(), 2);
    assert!(!sql.contains("to authenticated;") && !sql.contains("to anon;"));
    // A hole with dedupe has the events table, and answers a repeat before anything else runs.
    assert_eq!(
        sql.matches("create table \"svetra\".\"webhook_events\"")
            .count(),
        1
    );
    let at = sql
        .find("on conflict do nothing; if not found then return;")
        .expect("dedupe");
    assert!(
        at < sql
            .find("insert into \"svetra\".\"audit_log\"")
            .expect("audit")
    );
    // The one without dedupe never touches the events table.
    let form = &sql[sql
        .find("create function \"public\".\"form_submitted\"")
        .unwrap()..];
    assert!(!form[..form.find("revoke all").unwrap()].contains("webhook_events"));
}

#[test]
fn the_function_is_reached_by_the_sender_so_config_toml_says_no_token_is_needed() {
    let d = std::env::temp_dir().join(format!("svetra-wh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("supabase")).unwrap();
    std::fs::write(d.join("supabase/config.toml"), "[api]\nenabled = true\n").unwrap();
    std::fs::write(
        d.join("svetra.yaml"),
        include_str!("fixtures/webhooks.yaml"),
    )
    .unwrap();
    let run = svetra::project::compile(&d.join("svetra.yaml"), false).expect("compiles");
    assert!(run.config_updated);
    let config = std::fs::read_to_string(d.join("supabase/config.toml")).unwrap();
    assert!(config.contains("[functions.payment_received]\nverify_jwt = false"));
    assert!(config.contains("[functions.form_submitted]\nverify_jwt = false"));
    assert!(config.starts_with("[api]\nenabled = true\n"));
    // Run again: nothing changes, and --check agrees.
    let again = svetra::project::compile(&d.join("svetra.yaml"), true).expect("checks");
    assert!(again.stale.is_empty(), "{:?}", again.stale);
    // Edit the generated function by hand and --check says so.
    std::fs::write(d.join("supabase/functions/payment_received/index.ts"), "//").unwrap();
    let edited = svetra::project::compile(&d.join("svetra.yaml"), true).expect("checks");
    assert_eq!(
        edited.stale,
        ["supabase/functions/payment_received/index.ts"]
    );
    let _ = std::fs::remove_dir_all(&d);
}
