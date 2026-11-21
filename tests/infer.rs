use serde_json::{Value, json};
use std::process::Command;
use svetra::inspect::{infer_query, query};

const SNAPSHOT: &str = "tests/fixtures/infer/shop_app.json";

fn svetra(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(args)
        .output()
        .unwrap()
}

fn draft() -> String {
    let out = svetra(&["infer", "--snapshot", SNAPSHOT, "--out", "-"]);
    assert_eq!(out.status.code(), Some(0));
    String::from_utf8(out.stdout).unwrap()
}

/// The snapshot was taken from tests/pg/infer/shop_app.sql applied to PostgreSQL 16 with the
/// Supabase stand-in (tests/pg/supabase_stub.sql), through `svetra infer --print-query`.
#[test]
fn the_draft_matches_the_golden_file() {
    assert_eq!(draft(), include_str!("golden/shop_app.infer.yaml"));
}

#[test]
fn the_draft_is_the_same_every_time() {
    assert_eq!(draft(), draft());
}

/// The draft is valid input: it loads with no diagnostics, and validates with no errors. What
/// `validate` still says is documented in docs/infer.md (no policies, so SVT004 and SVT009).
#[test]
fn the_draft_loads_and_validates() {
    let yaml = draft();
    let (doc, diags) = svetra::load::load(&yaml);
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(doc.entities.len(), 4);
    let all = svetra::report::validate(&yaml);
    assert!(!svetra::report::has_errors(&all), "{all:?}");
    let mut codes: Vec<String> = all.iter().map(|d| format!("{:?}", d.code)).collect();
    codes.sort();
    codes.dedup();
    assert_eq!(codes, ["Svt004", "Svt009", "Svt010"], "{all:?}");
}

#[test]
fn the_draft_compiles() {
    let (doc, diags) = svetra::load::load(&draft());
    assert!(diags.is_empty(), "{diags:?}");
    let sql = svetra::compile::sql::schema(&doc);
    for table in ["order_items", "orders", "products", "profiles"] {
        assert!(sql.contains(&format!("\"public\".\"{table}\"")), "{table}");
    }
    assert!(
        sql.contains("references \"auth\".\"users\""),
        "the user field keeps its key"
    );
}

#[test]
fn everything_it_cannot_say_is_a_comment_with_the_original_sql() {
    let yaml = draft();
    for needle in [
        "# UNTRANSLATED column rating numeric: numeric without precision and scale",
        "# UNTRANSLATED column tags text[]",
        "# UNTRANSLATED check stock_text: CHECK (((stock)::text <> '-1'::text))",
        "# UNTRANSLATED check order_items_qty_check: CHECK (((qty > 0) AND (qty < LEAST(10, 100))))",
        "# UNTRANSLATED index orders_note_idx: CREATE INDEX orders_note_idx ON public.orders USING btree (lower(note))",
        "# UNTRANSLATED index products_tags_idx: CREATE INDEX products_tags_idx ON public.products USING gin (tags)",
        "# UNTRANSLATED foreign key profiles_id_fkey: FOREIGN KEY (id) REFERENCES auth.users(id)",
        "# UNTRANSLATED table public.legacy_codes: no primary key",
        "# UNTRANSLATED table public.membership: an entity needs one primary field",
        "# UNTRANSLATED identity (always)",
    ] {
        assert!(yaml.contains(needle), "missing: {needle}");
    }
    assert!(yaml.contains("# 10 things left as UNTRANSLATED"), "{yaml}");
    assert!(
        !yaml.contains("product_names") || yaml.contains("public.product_names: "),
        "views are only in the findings"
    );
}

#[test]
fn check_findings_are_comments_and_never_change_the_exit_code() {
    let yaml = draft();
    assert!(yaml.contains("# svetra check found 4 critical, 2 warnings and 0 notes:"));
    assert!(yaml.contains("#   critical  public.orders: "));
    assert!(yaml.contains("    # check: critical, rls_off: "));
    assert!(yaml.contains("# 4 entities from 6 tables. Skipped: 1 view, 0 functions."));
    // Same snapshot, same findings from `check`.
    let check = svetra(&["check", "--snapshot", SNAPSHOT]);
    assert_eq!(check.status.code(), Some(1));
}

#[test]
fn it_writes_the_file_and_says_what_it_did() {
    let dir = std::env::temp_dir().join(format!("svetra-infer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("svetra.yaml");
    let f = file.to_str().unwrap();
    let out = svetra(&["infer", "--snapshot", SNAPSHOT, "--out", f]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!(
            "wrote {f}: 4 entities, 22 fields, 10 things left as UNTRANSLATED, 4 critical findings in comments\nNext: review the draft, then `svetra validate`, then `svetra compile`.\n"
        )
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), draft());
    // A design you've edited is never lost.
    std::fs::write(&file, "mine").unwrap();
    let again = svetra(&["infer", "--snapshot", SNAPSHOT, "--out", f]);
    assert_eq!(again.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&again.stderr).contains("--force"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
    let forced = svetra(&["infer", "--snapshot", SNAPSHOT, "--out", f, "--force"]);
    assert_eq!(forced.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), draft());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_snapshot_from_check_is_refused_with_the_way_forward() {
    let out = svetra(&[
        "infer",
        "--snapshot",
        "tests/fixtures/check/leaky_app.json",
        "--out",
        "-",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("predates `infer`") && err.contains("svetra infer --print-query"),
        "{err}"
    );
}

#[test]
fn usage_errors_exit_2() {
    assert_eq!(svetra(&["infer"]).status.code(), Some(2));
    assert_eq!(svetra(&["infer", "--json"]).status.code(), Some(2));
    assert_eq!(svetra(&["check", "--out", "x"]).status.code(), Some(2));
    assert_eq!(svetra(&["infer", "--snapshot"]).status.code(), Some(2));
}

#[test]
fn the_infer_query_extends_check_s_without_changing_it() {
    let schemas = vec!["public".to_string()];
    let check = query(&schemas);
    let infer = infer_query(&schemas);
    let keys = check.strip_suffix(')').unwrap();
    assert!(
        infer.starts_with(keys),
        "check's keys come first and are unchanged"
    );
    for key in ["'columns'", "'constraints'", "'indexes'", "'enums'"] {
        assert!(infer.contains(key) && !check.contains(key), "{key}");
    }
    let out = svetra(&["infer", "--print-query"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("{infer};\n"));
    let out = svetra(&["check", "--print-query"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("{check};\n"));
}

fn tiny(tables: Value, columns: Value, constraints: Value) -> Value {
    json!({ "tables": tables, "policies": [], "views": [], "functions": [],
            "columns": columns, "constraints": constraints, "indexes": [], "enums": [] })
}

fn col(table: &str, name: &str, ty: &str, not_null: bool) -> Value {
    json!({ "schema": "public", "table": table, "name": name, "type": ty, "type_schema": "pg_catalog",
            "type_name": ty, "kind": "b", "not_null": not_null, "default": null, "identity": null, "generated": false })
}

fn table(name: &str) -> Value {
    json!({ "schema": "public", "name": name, "rls": true,
            "anon": {"select": false, "insert": false, "update": false, "delete": false},
            "authenticated": {"select": false, "insert": false, "update": false, "delete": false} })
}

fn pk(table: &str, column: &str) -> Value {
    json!({ "schema": "public", "table": table, "name": format!("{table}_pkey"), "kind": "p",
            "columns": [column], "definition": format!("PRIMARY KEY ({column})") })
}

#[test]
fn names_and_foreign_keys_it_cannot_express_stay_explicit() {
    let snap = tiny(
        json!([table("Order"), table("items"), table("auth_logs")]),
        json!([
            col("Order", "id", "uuid", true),
            col("items", "id", "uuid", true),
            col("items", "order", "uuid", false),
            col("items", "Weird", "text", false),
            col("items", "note", "text", false),
            col("auth_logs", "id", "uuid", true),
            col("auth_logs", "who", "uuid", false),
        ]),
        json!([
            pk("Order", "id"),
            pk("items", "id"),
            pk("auth_logs", "id"),
            { "schema": "public", "table": "items", "name": "items_order_fkey", "kind": "f",
              "columns": ["order"], "ref_schema": "public", "ref_table": "Order", "ref_columns": ["id"],
              "definition": "FOREIGN KEY (\"order\") REFERENCES \"Order\"(id)" },
            { "schema": "public", "table": "auth_logs", "name": "auth_logs_who_fkey", "kind": "f",
              "columns": ["who"], "ref_schema": "auth", "ref_table": "users", "ref_columns": ["id"],
              "definition": "FOREIGN KEY (who) REFERENCES auth.users(id) ON DELETE SET NULL" },
            { "schema": "public", "table": "items", "name": "items_note_check", "kind": "c",
              "columns": ["note"], "definition": "CHECK ((note <> ''::text AND\n   length(note) > 1))" },
        ]),
    );
    let d = svetra::infer::infer(&snap).unwrap();
    let y = &d.yaml;
    assert!(
        y.contains("# UNTRANSLATED table public.Order: its name doesn't compile back"),
        "{y}"
    );
    assert!(
        y.contains("order: { type: uuid }"),
        "an unreachable reference stays a plain uuid"
    );
    assert!(y.contains("# UNTRANSLATED foreign key items_order_fkey: FOREIGN KEY (\"order\") REFERENCES \"Order\"(id)"));
    assert!(y.contains("# UNTRANSLATED column \"Weird\" text: its name isn't snake_case"));
    assert!(y.contains(
        "who: { type: user }   # lossy: the foreign key had ON DELETE SET NULL, Svetra writes none"
    ));
    assert!(
        y.contains(
            "# UNTRANSLATED check items_note_check: CHECK ((note <> ''::text AND length(note) > 1))"
        ),
        "SQL is put on one line so a comment can't end early: {y}"
    );
    assert!(
        svetra::report::validate(y)
            .iter()
            .all(|d| d.severity() != svetra::diag::Severity::Error)
    );
    assert_eq!((d.entities, d.fields, d.untranslated), (2, 5, 4));
}

#[test]
fn nothing_to_infer_is_an_error() {
    let snap = tiny(
        json!([table("Order")]),
        json!([col("Order", "id", "uuid", true)]),
        json!([pk("Order", "id")]),
    );
    let err = svetra::infer::infer(&snap).unwrap_err();
    assert!(err.starts_with("Nothing to infer"), "{err}");
}
