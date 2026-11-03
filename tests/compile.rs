use svetra::compile::sql::schema;
use svetra::load::load;

fn compile(src: &str) -> String {
    let (doc, diags) = load(src);
    assert!(diags.is_empty(), "{diags:#?}");
    schema(&doc)
}

/// Golden files are compared byte for byte. It was checked by applying it to Postgres 16 and
/// confirming that every constraint rejects bad rows. To update it after an intended change:
/// regenerate it, review the diff, and apply it to Postgres again.
#[test]
fn ledger_sql_matches_golden_file() {
    let got = compile(include_str!("fixtures/finledger.yaml"));
    assert_eq!(got, include_str!("golden/finledger.sql"));
}

/// Covers private entities, multi-action policies and an `anon` policy. Checked on Postgres 16:
/// anon can read posts but not the audit log, and authors can't insert posts as someone else.
#[test]
fn posts_sql_matches_golden_file() {
    let got = compile(include_str!("fixtures/posts.yaml"));
    assert_eq!(got, include_str!("golden/posts.sql"));
}

#[test]
fn every_table_gets_rls_and_private_tables_lose_api_grants() {
    let sql = compile(include_str!("fixtures/posts.yaml"));
    assert!(sql.contains("alter table \"public\".\"post\" enable row level security;"));
    assert!(sql.contains("alter table \"public\".\"audit_log\" enable row level security;"));
    assert!(sql.contains("revoke all on table \"public\".\"audit_log\" from anon, authenticated;"));
    assert!(!sql.contains("revoke all on table \"public\".\"post\""));
}

#[test]
fn policies_split_by_action_with_the_right_clauses() {
    let sql = compile(include_str!("fixtures/posts.yaml"));
    for action in ["select", "insert", "update", "delete"] {
        assert!(
            sql.contains(&format!("create policy \"posts_by_author_{action}\"")),
            "{action}"
        );
    }
    assert!(sql.contains("for insert to authenticated\n  with check ("));
    assert!(sql.contains(
        "for update to authenticated\n  using (\"author\" = (select auth.uid()))\n  with check ("
    ));
    assert!(sql.contains("create policy \"public_read\" on \"public\".\"post\"\n  for select to anon\n  using (true);"));
}

#[test]
fn output_is_deterministic() {
    let src = include_str!("fixtures/finledger.yaml");
    assert_eq!(compile(src), compile(src));
}

const HEAD: &str = "svetra: \"0.1\"\nsystem: App\n";

#[test]
fn reserved_words_are_quoted_and_cycles_use_late_foreign_keys() {
    let sql = compile(&format!(
        "{HEAD}entities:\n  Order:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      user: {{ type: ref, entity: User }}\n\
         \x20 User:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      last_order: {{ type: ref, entity: Order }}\n"
    ));
    assert!(sql.contains("create table \"public\".\"order\" ("));
    assert!(sql.contains("create table \"public\".\"user\" ("));
    let tables_end = sql.find("-- Foreign keys").unwrap();
    assert!(
        !sql[..tables_end].contains("references"),
        "foreign keys come after all tables"
    );
    assert!(sql.contains("alter table \"public\".\"user\" add constraint \"user_last_order_fkey\" foreign key (\"last_order\") references \"public\".\"order\" (\"id\");"));
}

#[test]
fn reference_and_user_fields_get_an_index_unless_one_leads() {
    let sql = compile(&format!(
        "{HEAD}entities:\n  A:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      owner: {{ type: user }}\n      parent: {{ type: ref, entity: A }}\n      code: {{ type: text, unique: true }}\n    indexes:\n      - [parent, code]\n"
    ));
    assert!(sql.contains("create index \"a_owner_idx\" on \"public\".\"a\" (\"owner\");"));
    assert!(sql.contains("create index \"a_parent_code_idx\""));
    assert!(
        !sql.contains("\"a_parent_idx\""),
        "the declared index already leads with parent"
    );
    assert!(sql.contains("constraint \"a_code_key\" unique (\"code\")"));
}

#[test]
fn defaults_and_non_uuid_primary_keys() {
    let sql = compile(&format!(
        "{HEAD}entities:\n  Plan:\n    fields:\n      code: {{ type: text, primary: true }}\n      seats: {{ type: int, required: true, default: 1 }}\n      active: {{ type: bool, default: true }}\n      note: {{ type: text, default: \"it's\" }}\n"
    ));
    assert!(sql.contains("\"code\" text primary key,"));
    assert!(sql.contains("\"seats\" bigint not null default 1,"));
    assert!(sql.contains("\"active\" boolean default true,"));
    assert!(sql.contains("\"note\" text default 'it''s'"));
}
