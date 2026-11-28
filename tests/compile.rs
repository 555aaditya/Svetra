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
    assert!(sql.contains("create policy \"public_read\" on \"public\".\"post\"\n  for select to anon, authenticated\n  using (true);"));
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

fn types(src: &str) -> String {
    let (doc, diags) = load(src);
    assert!(diags.is_empty(), "{diags:#?}");
    svetra::compile::ts::types(&doc)
}

/// Checked with `tsc --strict` and run against Zod 3.25 and Zod 4.6: bad enums, uuids and decimal
/// shapes are rejected, numbers from Supabase become exact strings, and numbers JavaScript has
/// already rounded are rejected.
#[test]
fn ledger_types_match_golden_file() {
    assert_eq!(
        types(include_str!("fixtures/finledger.yaml")),
        include_str!("golden/finledger.ts")
    );
}

#[test]
fn posts_types_match_golden_file_and_skip_the_decimal_helper() {
    let ts = types(include_str!("fixtures/posts.yaml"));
    assert_eq!(ts, include_str!("golden/posts.ts"));
    assert!(
        !ts.contains("const decimal"),
        "no decimal fields, so no helper"
    );
}

#[test]
fn optional_fields_are_nullable_and_types_map_as_specified() {
    let ts = types(&format!(
        "{HEAD}entities:\n  T:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      n: {{ type: int, required: true }}\n      f: {{ type: float }}\n      d: {{ type: date }}\n      j: {{ type: json }}\n      ok: {{ type: bool, required: true }}\n      price: {{ type: decimal, precision: 10, scale: 0 }}\n"
    ));
    assert!(ts.contains("  n: z.number().int(),"));
    assert!(ts.contains("  f: z.number().nullable(),"));
    assert!(ts.contains("  d: z.string().date().nullable(),"));
    assert!(ts.contains("  j: z.unknown().nullable(),"));
    assert!(ts.contains("  ok: z.boolean(),"));
    assert!(ts.contains("  price: decimal(10, 0).nullable(),"));
    assert!(ts.contains("export type T = z.infer<typeof T>;"));
}

#[test]
fn as_anyone_covers_signed_in_and_logged_out_people() {
    let src = format!(
        "{HEAD}entities:\n  Post:\n    fields:\n      id: {{ type: uuid, primary: true }}\n      author: {{ type: user, required: true }}\n    indexes:\n      - [author]\n\
         policies:\n  read_all:\n    on: Post\n    allow: [select]\n    when: \"true\"\n    as: anyone\n  write_own:\n    on: Post\n    allow: [insert]\n    when: \"author == auth.user\"\n"
    );
    let sql = compile(&src);
    assert!(sql.contains("create policy \"read_all\" on \"public\".\"post\"\n  for select to anon, authenticated\n  using (true);"));
    let (doc, diags) = load(&src);
    let text = svetra::understand::explain::explain(&doc, &diags);
    assert!(
        text.contains("Anyone, signed in or not, can see posts."),
        "{text}"
    );
}

/// Roles: checked on Postgres 16 by scripts/pg-check.sh as customers, staff at two locations, an
/// admin, a user with no role and a logged-out visitor, with a readback after every write.
#[test]
fn bookings_and_teams_sql_and_types_match_golden_files() {
    let src = include_str!("fixtures/bookings.yaml");
    assert_eq!(compile(src), include_str!("golden/bookings.sql"));
    assert_eq!(types(src), include_str!("golden/bookings.ts"));
    let src = include_str!("fixtures/teams.yaml");
    assert_eq!(compile(src), include_str!("golden/teams.sql"));
    assert_eq!(types(src), include_str!("golden/teams.ts"));
    let src = include_str!("fixtures/approvals.yaml");
    assert_eq!(compile(src), include_str!("golden/approvals.sql"));
    assert_eq!(types(src), include_str!("golden/approvals.ts"));
}

/// Every `on_delete` option, with a chain of cascades and a self reference. Checked on
/// PostgreSQL 16 by `scripts/pg-check.sh`, with the delete checks it generates.
#[test]
fn forum_sql_matches_golden_file() {
    let got = compile(include_str!("fixtures/forum.yaml"));
    assert_eq!(got, include_str!("golden/forum.sql"));
}

#[test]
fn on_delete_becomes_the_foreign_keys_action() {
    let sql = compile(include_str!("fixtures/forum.yaml"));
    // The default writes nothing, and a `user` key never cascades.
    assert!(sql.contains(
        "\"post_thread_fkey\" foreign key (\"thread\") references \"public\".\"thread\" (\"id\");"
    ));
    assert!(sql.contains("\"thread_board_fkey\" foreign key (\"board\") references \"public\".\"board\" (\"id\") on delete cascade;"));
    assert!(sql.contains("\"post_reply_to_fkey\" foreign key (\"reply_to\") references \"public\".\"post\" (\"id\") on delete set null;"));
    assert!(sql.contains(
        "\"post_owner_fkey\" foreign key (\"owner\") references \"auth\".\"users\" (\"id\");"
    ));
}

/// A table keyed by the auth user: the key is a uuid with a foreign key to `auth.users`.
#[test]
fn profiles_sql_matches_golden_file() {
    let got = compile(include_str!("fixtures/profiles.yaml"));
    assert_eq!(got, include_str!("golden/profiles.sql"));
}

/// Checked on Postgres 16 by `scripts/pg-check.sh` (`tests/pg/audited_assert.sql`): row changes and
/// hole calls are logged with the actor, and the log is closed to the app and append-only.
#[test]
fn audited_sql_matches_golden_file() {
    let got = compile(include_str!("fixtures/audited.yaml"));
    assert_eq!(got, include_str!("golden/audited.sql"));
}

/// Checked on Postgres 16 by `scripts/pg-check.sh` (`tests/pg/validated_assert.sql`): bad input is
/// refused with 22023 after the guard, and a caller the guard refuses never sees that error.
#[test]
fn validated_sql_matches_golden_file() {
    let got = compile(include_str!("fixtures/validated.yaml"));
    assert_eq!(got, include_str!("golden/validated.sql"));
}

#[test]
fn require_comes_after_the_guard_and_reads_the_condition_as_text() {
    let sql = compile(include_str!("fixtures/validated.yaml"));
    let guard = sql.find("not allowed to call deposit").expect("guard");
    let first = sql
        .find("using message = 'deposit: amount > 0'")
        .expect("first requirement");
    let second = sql
        .find("using message = 'deposit: amount <= 10000'")
        .expect("second requirement");
    assert!(guard < first && first < second);
    // Quotes are doubled, and `using message` means the `%` is not a placeholder.
    assert!(sql.contains("label != ''100%''"));
}

#[test]
fn a_hole_without_require_compiles_as_before() {
    let sql = compile(include_str!("fixtures/finledger.yaml"));
    assert!(!sql.contains("22023"));
}

#[test]
fn audit_output_appears_only_when_something_is_audited() {
    for src in [
        include_str!("fixtures/finledger.yaml"),
        include_str!("fixtures/posts.yaml"),
    ] {
        let sql = compile(src);
        assert!(
            !sql.contains("\"svetra\".\"audit_log\""),
            "unexpected audit log"
        );
        assert!(!sql.contains("-- Audit"));
    }
    let sql = compile(include_str!("fixtures/audited.yaml"));
    assert_eq!(sql.matches("create trigger \"svetra_audit\" ").count(), 1);
    assert!(sql.contains("-- Audit\n"));
}

#[test]
fn an_audited_hole_logs_in_the_wrapper_and_never_in_its_own_role() {
    let sql = compile(include_str!("fixtures/audited.yaml"));
    // Both holes are audited; their roles get no grant on the log.
    assert_eq!(sql.matches("'call', 'archive'").count(), 1);
    assert!(sql.contains("jsonb_build_object('note', note)"));
    assert!(sql.contains("'call', 'sweep', 'call', '{}'::jsonb"));
    assert!(!sql.contains(
        "to \"svetra_hole_archive\";\ncreate policy \"svetra_hole_archive\" on \"svetra\""
    ));
    assert!(!sql.contains("on table \"svetra\".\"audit_log\" to"));
}

#[test]
fn an_unaudited_hole_has_the_plain_wrapper() {
    let sql = compile(include_str!("fixtures/posts.yaml"));
    assert!(sql.contains("as $svetra$ begin perform"));
}

/// A scheduled hole: a pg_cron job, no function in `public`, nothing in the TypeScript client.
/// Checked on PostgreSQL 16 by `scripts/pg-check.sh` and on a real local Supabase by
/// `scripts/supabase-check.sh`.
#[test]
fn jobs_sql_and_types_match_golden_files() {
    let src = include_str!("fixtures/jobs.yaml");
    assert_eq!(compile(src), include_str!("golden/jobs.sql"));
    assert_eq!(types(src), include_str!("golden/jobs.ts"));
}

/// A triggered hole: a trigger function and one trigger per event, no function in `public`,
/// nothing in the TypeScript client. Checked on PostgreSQL 16 by `scripts/pg-check.sh` and on a
/// real local Supabase by `scripts/supabase-check.sh`.
#[test]
fn triggers_sql_and_types_match_golden_files() {
    let src = include_str!("fixtures/triggers.yaml");
    assert_eq!(compile(src), include_str!("golden/triggers.sql"));
    assert_eq!(types(src), include_str!("golden/triggers.ts"));
}

/// Checked on Postgres 16 by `scripts/pg-check.sh` (`tests/pg/retries_assert.sql`): a repeat returns
/// the first answer, a changed input is refused, the guard runs again, and the key table is closed.
#[test]
fn retries_sql_matches_golden_file() {
    let got = compile(include_str!("fixtures/retries.yaml"));
    assert_eq!(got, include_str!("golden/retries.sql"));
}

#[test]
fn the_key_table_and_shared_guard_exist_only_for_retry_keys() {
    let sql = compile(include_str!("fixtures/retries.yaml"));
    assert_eq!(
        sql.matches("create table \"svetra\".\"hole_keys\"").count(),
        1
    );
    // One shared guard for the hole that has `when` and a key; none for those without.
    assert!(sql.contains("\"svetra_holes\".\"pay_allowed\"("));
    assert!(!sql.contains("relabel_allowed") && !sql.contains("total_allowed"));
    // The body function's own guard is that function, so the two can't drift apart.
    assert!(sql.contains("if not coalesce(\"svetra_holes\".\"pay_allowed\"(p_caller, p_request"));
    for src in [
        include_str!("fixtures/finledger.yaml"),
        include_str!("fixtures/validated.yaml"),
        include_str!("fixtures/audited.yaml"),
    ] {
        let sql = compile(src);
        assert!(!sql.contains("hole_keys") && !sql.contains("_allowed"));
    }
}

#[test]
fn a_repeat_checks_who_may_call_before_it_looks_at_the_inputs() {
    let sql = compile(include_str!("fixtures/retries.yaml"));
    let pay = &sql[sql.find("create function \"public\".\"pay\"").expect("pay")..];
    let guard = pay.find("pay_allowed").expect("guard");
    let inputs = pay.find("is distinct from v_inputs").expect("inputs");
    let replay = pay.find("jsonb_populate_record").expect("replay");
    assert!(guard < inputs && inputs < replay);
}
