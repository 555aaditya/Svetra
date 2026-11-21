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
/// Supabase stand-in (src/standin/base.sql), through `svetra infer --print-query`.
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
    assert_eq!(doc.entities.len(), 5);
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
        "# UNTRANSLATED table public.legacy_codes: no primary key",
        "# UNTRANSLATED table public.membership: an entity needs one primary field",
        "# UNTRANSLATED identity (always)",
    ] {
        assert!(yaml.contains(needle), "missing: {needle}");
    }
    assert!(yaml.contains("# 12 things left as UNTRANSLATED"), "{yaml}");
    assert!(
        !yaml.contains("product_names") || yaml.contains("public.product_names: "),
        "views are only in the findings"
    );
}

#[test]
fn check_findings_are_comments_and_never_change_the_exit_code() {
    let yaml = draft();
    assert!(yaml.contains("# svetra check found 3 critical, 2 warnings and 0 notes:"));
    assert!(yaml.contains("#   critical  public.order_items: "));
    assert!(yaml.contains("    # check: critical, rls_off: "));
    assert!(yaml.contains("# 5 entities from 7 tables. Skipped: 1 view, 0 functions."));
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
            "wrote {f}: 5 entities, 27 fields, 12 things left as UNTRANSLATED, 3 critical findings in comments\nNext: review the draft, then `svetra validate`, then `svetra compile`.\n"
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
        "who: { type: user }   # lossy: the foreign key had ON DELETE SET NULL, a user field has none"
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

fn snapshot() -> Value {
    serde_json::from_str(&std::fs::read_to_string(SNAPSHOT).unwrap()).unwrap()
}

/// The draft of a changed snapshot, loaded and validated; its policies as `(name, entity)`.
fn changed(edit: impl Fn(&mut Value)) -> (String, Vec<(String, String)>) {
    let mut snap = snapshot();
    edit(&mut snap);
    let yaml = svetra::infer::infer(&snap).unwrap().yaml;
    let (doc, diags) = svetra::load::load(&yaml);
    assert!(diags.is_empty(), "{diags:?}");
    assert!(!svetra::report::has_errors(&svetra::report::validate(
        &yaml
    )));
    let policies = doc
        .policies
        .iter()
        .map(|p| (p.name.clone(), p.on.name.clone()))
        .collect();
    (yaml, policies)
}

fn policy_row<'a>(snap: &'a mut Value, name: &str) -> &'a mut Value {
    snap["policies"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["name"] == name)
        .unwrap()
}

#[test]
fn owner_and_true_policies_translate() {
    let yaml = draft();
    assert!(yaml.contains("# Policies: 12 of 15 translated."));
    for block in [
        "  orders_select_own:\n    on: Orders\n    as: authenticated\n    allow: [select]\n    when: \"customer == auth.user\"\n",
        "  orders_insert_own:\n    on: Orders\n    as: authenticated\n    allow: [insert]\n    when: \"customer == auth.user\"\n",
        "  orders_update_own:\n    on: Orders\n    as: authenticated\n    allow: [update]\n    when: \"customer == auth.user\"\n",
        "  orders_delete_guard:\n    on: Orders\n    as: authenticated\n    allow: [delete]\n    restrictive: true\n",
        "  products_read:\n    on: Products\n    as: anyone\n    allow: [select]\n    when: \"true\"\n",
    ] {
        assert!(yaml.contains(block), "missing: {block}");
    }
}

#[test]
fn a_partly_translated_table_says_the_draft_is_narrower() {
    let yaml = draft();
    let orders = yaml.split("\n  Orders:\n").nth(1).unwrap();
    let orders = orders.split("\n  Products:").next().unwrap();
    assert!(
        orders.contains("# policies: 5 of 6 translated. The draft is NARROWER than the database")
    );
    assert!(!orders.contains("private: true"));
    assert!(orders.contains("# UNTRANSLATED policy orders_staff_read: CREATE POLICY orders_staff_read ON public.orders AS PERMISSIVE FOR SELECT TO authenticated USING (((auth.jwt() ->> 'role'::text) = 'staff'::text))"));
}

#[test]
fn no_policy_is_invented_or_drafted_where_rls_is_off() {
    let (yaml, policies) = changed(|_| {});
    let items = yaml
        .split("  OrderItems:")
        .nth(1)
        .unwrap()
        .split("  Orders:")
        .next()
        .unwrap();
    assert!(items.contains("row-level security is off here"));
    assert!(items.contains("private: true"));
    assert!(items.contains("# UNTRANSLATED policy order_items_via_order"));
    assert!(policies.iter().all(|(_, on)| on != "OrderItems"));
    // Even a translatable policy isn't drafted when it isn't enforced.
    let (_, policies) = changed(|snap| {
        policy_row(snap, "order_items_via_order")["using"] = Value::String("true".into());
    });
    assert!(policies.iter().all(|(_, on)| on != "OrderItems"));
}

#[test]
fn a_table_where_nothing_translated_stays_private() {
    let (yaml, policies) = changed(|snap| {
        for name in ["profiles_own", "profiles_insert_own"] {
            let p = policy_row(snap, name);
            for key in ["using", "check"] {
                if p[key].is_string() {
                    p[key] = Value::String("is_admin()".into());
                }
            }
        }
    });
    assert!(policies.iter().all(|(_, on)| on != "Profiles"));
    let profiles = yaml.split("  Profiles:").nth(1).unwrap();
    assert!(profiles.contains("# policies: none of 2 translated."));
    assert!(profiles.contains("private: true"));
}

#[test]
fn a_restrictive_policy_that_does_not_translate_removes_the_whole_table() {
    let (yaml, policies) = changed(|snap| {
        policy_row(snap, "orders_delete_guard")["using"] = Value::String("is_admin()".into());
    });
    assert!(
        policies.iter().all(|(_, on)| on != "Orders"),
        "{policies:?}"
    );
    let orders = yaml.split("\n  Orders:\n").nth(1).unwrap();
    let orders = orders.split("\n  Products:").next().unwrap();
    assert!(orders.contains("private: true"));
    assert!(orders.contains("a restrictive policy on this table didn't translate"));
    assert!(orders.contains("# UNTRANSLATED policy orders_select_own"));
}

#[test]
fn restrictive_policies_alone_are_not_drafted() {
    let (_, policies) = changed(|snap| {
        for name in [
            "orders_select_own",
            "orders_update_own",
            "orders_delete_own",
            "orders_insert_own",
        ] {
            let p = policy_row(snap, name);
            if p["using"].is_string() {
                p["using"] = Value::String("is_admin()".into());
            }
            if p["check"].is_string() {
                p["check"] = Value::String("is_admin()".into());
            }
        }
    });
    assert!(
        policies.iter().all(|(_, on)| on != "Orders"),
        "{policies:?}"
    );
}

#[test]
fn an_action_the_database_does_not_grant_is_not_allowed() {
    let (_, policies) = changed(|snap| {
        for t in snap["tables"].as_array_mut().unwrap() {
            if t["name"] == "orders" {
                t["authenticated"]["delete"] = Value::Bool(false);
            }
        }
    });
    // The restrictive delete guard lost its only action, so it isn't translated, and dropping a
    // restrictive policy would widen access: the whole table goes.
    assert!(
        policies.iter().all(|(_, on)| on != "Orders"),
        "{policies:?}"
    );
}

#[test]
fn a_permissive_policy_without_its_privilege_only_narrows() {
    let (yaml, policies) = changed(|snap| {
        for t in snap["tables"].as_array_mut().unwrap() {
            if t["name"] == "orders" {
                t["authenticated"]["insert"] = Value::Bool(false);
            }
        }
    });
    assert!(!policies.iter().any(|(n, _)| n == "orders_insert_own"));
    assert!(policies.iter().any(|(n, _)| n == "orders_select_own"));
    assert!(yaml.contains("# policies: 4 of 6 translated."));
    assert!(yaml.contains("withholds the table privilege"));
}

#[test]
fn the_translated_policies_compile() {
    let (doc, diags) = svetra::load::load(&draft());
    assert!(diags.is_empty(), "{diags:?}");
    let sql = svetra::compile::sql::schema(&doc);
    assert!(sql.contains("orders_select_own"), "{sql}");
    assert!(sql.contains("as restrictive"), "{sql}");
    assert!(sql.contains("(select auth.uid())"));
    assert!(sql.contains("anon, authenticated"));
    assert!(!sql.contains("order_items_via_order"));
}

fn entity_block<'a>(yaml: &'a str, name: &str) -> &'a str {
    let at = yaml.find(&format!("\n  {name}:\n")).unwrap() + 1;
    let rest = &yaml[at..];
    let end = rest
        .match_indices('\n')
        .map(|(i, _)| i + 1)
        .find(|&i| !rest[i..].starts_with("   "))
        .unwrap_or(rest.len());
    &rest[..end]
}

/// A table keyed by the auth user's id is a `user` key, so its own `id = auth.uid()` is an owner
/// check, and a column that points at it holds the same person's id.
#[test]
fn a_profile_table_and_ownership_through_it() {
    let yaml = draft();
    let profiles = entity_block(&yaml, "Profiles");
    assert!(
        profiles.contains("id:           { type: user, primary: true }"),
        "{profiles}"
    );
    assert!(!profiles.contains("UNTRANSLATED foreign key"), "{profiles}");
    assert!(!profiles.contains("private: true"), "{profiles}");
    for block in [
        "  profiles_own:\n    on: Profiles\n    as: authenticated\n    allow: [select]\n    when: \"id == auth.user\"\n",
        "  profiles_insert_own:\n    on: Profiles\n    as: authenticated\n    allow: [insert]\n    when: \"id == auth.user\"\n",
        "  reviews_insert_own:\n    on: Reviews\n    as: authenticated\n    allow: [insert]\n    when: \"author == auth.user\"\n",
        "  reviews_delete_own:\n    on: Reviews\n    as: authenticated\n    allow: [delete]\n    when: \"author == auth.user\"\n",
    ] {
        assert!(yaml.contains(block), "missing: {block}");
    }
    // The column that pointed at the profile table is a `user` field, and says what that lost.
    assert!(
        yaml.contains("author:     { type: user, required: true }   # lossy: was a foreign key to public.profiles (id), now one to auth.users: it accepts a person with no row in profiles; the foreign key had ON DELETE CASCADE, a user field has none"),
        "{yaml}"
    );
}

#[test]
fn only_one_hop_through_a_profile_table_is_followed() {
    // Drop the profile key's link to auth.users: `author` is then an ordinary reference.
    let (yaml, policies) = changed(|snap| {
        let cons = snap["constraints"].as_array_mut().unwrap();
        cons.retain(|c| c["name"] != "profiles_id_fkey");
    });
    let profiles = entity_block(&yaml, "Profiles");
    assert!(
        profiles.contains("id:           { type: uuid, primary: true }"),
        "{profiles}"
    );
    assert!(
        yaml.contains(
            "author:     { type: ref, entity: Profiles, on_delete: cascade, required: true }"
        ),
        "{yaml}"
    );
    assert!(!policies.iter().any(|(n, _)| n == "reviews_insert_own"));
    assert!(
        yaml.contains("`author` is a reference to Profiles, not a user field"),
        "{yaml}"
    );
    assert!(
        yaml.contains(
            "`id` is a plain uuid column, not a user field (it has no foreign key to auth.users)"
        ),
        "{yaml}"
    );
}

#[test]
fn a_cascading_key_becomes_on_delete() {
    let yaml = draft();
    assert!(
        yaml.contains(
            "product_id: { type: ref, entity: Products, on_delete: cascade, required: true }"
        ),
        "{yaml}"
    );
    assert!(
        yaml.contains("parent_id:  { type: ref, entity: Reviews, on_delete: set_null }"),
        "{yaml}"
    );
    // The default action writes nothing, and one Svetra can't write is a note.
    assert!(
        yaml.contains("order_id:   { type: ref, entity: Orders, required: true }"),
        "{yaml}"
    );
    let (yaml, _) = changed(|snap| {
        for c in snap["constraints"].as_array_mut().unwrap() {
            if c["name"] == "reviews_parent_id_fkey" {
                c["definition"] = Value::String(
                    "FOREIGN KEY (parent_id) REFERENCES reviews(id) ON UPDATE CASCADE ON DELETE SET DEFAULT".into(),
                );
            }
        }
    });
    assert!(
        yaml.contains("parent_id:  { type: ref, entity: Reviews }   # lossy: the foreign key had ON DELETE SET DEFAULT, which Svetra can't write; lossy: the foreign key had ON UPDATE CASCADE, Svetra writes none"),
        "{yaml}"
    );
}

#[test]
fn auth_role_reads_say_who_they_are_for() {
    let yaml = draft();
    assert!(
        yaml.contains("  reviews_read:\n    on: Reviews\n    as: authenticated\n    allow: [select]\n    when: \"true\"\n"),
        "{yaml}"
    );
    // `auth.role() = 'authenticated' and author = auth.uid()` is the owner check for that role.
    assert!(
        yaml.contains("  reviews_update_own:\n    on: Reviews\n    as: authenticated\n    allow: [update]\n    when: \"author == auth.user\"\n"),
        "{yaml}"
    );
    // Same read for everyone who may call it: anonymous people are still kept out.
    let (yaml, _) = changed(|snap| {
        policy_row(snap, "reviews_read")["roles"] = json!(["public"]);
    });
    assert!(
        yaml.contains("  reviews_read:\n    on: Reviews\n    as: authenticated\n"),
        "{yaml}"
    );
    // The role it can never be true for: not drafted, with the reason.
    let (yaml, policies) = changed(|snap| {
        policy_row(snap, "reviews_read")["using"] =
            Value::String("(auth.role() = 'service_role'::text)".into());
    });
    assert!(!policies.iter().any(|(n, _)| n == "reviews_read"));
    assert!(
        yaml.contains("-- auth.role() is compared with 'service_role', not 'authenticated'"),
        "{yaml}"
    );
}

#[test]
fn each_untranslated_policy_says_why() {
    let yaml = draft();
    assert!(yaml.contains("-- reads a JWT claim (auth.jwt())"), "{yaml}");
    assert!(yaml.contains("-- a subquery\n"), "{yaml}");
    assert!(yaml.contains("-- row-level security is off, so the policy isn't enforced"));
    assert!(!yaml.contains("isn't an owner check"));
    let (yaml, _) = changed(|snap| {
        policy_row(snap, "reviews_read")["using"] = Value::String("is_staff()".into());
    });
    assert!(
        yaml.contains("-- calls the function is_staff(); infer doesn't read function bodies"),
        "{yaml}"
    );
}
