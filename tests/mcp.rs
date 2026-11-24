use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

/// Send `messages` to `svetra mcp` in `dir`, close stdin, and return its replies by id.
fn session(dir: &std::path::Path, messages: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_svetra"))
        .arg("mcp")
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("start svetra mcp");
    let mut stdin = child.stdin.take().unwrap();
    for m in messages {
        writeln!(stdin, "{m}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("each reply is one JSON line"))
        .collect()
}

fn project(name: &str, yaml: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("svetra-mcp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("svetra.yaml"), yaml).unwrap();
    dir
}

fn call(id: i64, tool: &str, args: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": tool, "arguments": args } })
}

fn has_code(v: &Value, code: &str) -> bool {
    v["diagnostics"]
        .as_array()
        .is_some_and(|d| d.iter().any(|x| x["code"] == code))
}

/// A tool result's JSON text, and whether it was an error.
fn result(reply: &Value) -> (Value, bool) {
    let r = &reply["result"];
    let text = r["content"][0]["text"].as_str().expect("text content");
    (
        serde_json::from_str(text).unwrap(),
        r["isError"].as_bool().unwrap(),
    )
}

#[test]
fn handshake_lists_the_tools() {
    let dir = project("handshake", include_str!("fixtures/clinic.yaml"));
    let replies = session(
        &dir,
        &[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } } }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } }),
        ],
    );
    assert_eq!(
        replies.len(),
        4,
        "the notification gets no reply: {replies:?}"
    );
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "svetra");
    let names: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "validate", "compile", "explain", "context", "graph", "plan", "patch", "check",
            "doctor"
        ]
    );
    assert_eq!(replies[2]["result"], json!({}));
    assert_eq!(
        replies[3]["result"]["protocolVersion"], "2025-06-18",
        "an unknown version gets ours"
    );
}

#[test]
fn tools_validate_compile_and_explain() {
    let dir = project("tools", include_str!("fixtures/clinic.yaml"));
    std::fs::write(dir.join("leaky.yaml"), "svetra: \"0.1\"\nsystem: Shop\nentities:\n  Order:\n    fields:\n      id: { type: uuid, primary: true }\n").unwrap();
    let replies = session(
        &dir,
        &[
            call(1, "validate", json!({})),
            call(2, "validate", json!({ "file": "leaky.yaml" })),
            call(3, "compile", json!({})),
            call(4, "compile", json!({ "check": true })),
            call(5, "explain", json!({})),
            call(6, "context", json!({})),
            call(7, "graph", json!({})),
            call(8, "explain", json!({ "file": "leaky.yaml" })),
            call(9, "validate", json!({ "file": "missing.yaml" })),
            call(10, "graph", json!({ "file": "leaky.yaml" })),
        ],
    );
    let (v, err) = result(&replies[0]);
    assert!(!err && v["valid"] == true, "{v}");
    let (v, err) = result(&replies[1]);
    assert!(!err && v["valid"] == false && has_code(&v, "SVT001"), "{v}");
    let (v, err) = result(&replies[2]);
    assert!(!err, "{v}");
    assert_eq!(
        v["written"],
        json!([
            "supabase/schemas/svetra.sql",
            "src/svetra.gen.ts",
            "supabase/tests/svetra.test.sql"
        ])
    );
    assert_eq!(v["no_config"], true);
    assert!(dir.join("supabase/schemas/svetra.sql").exists());
    let (v, _) = result(&replies[3]);
    assert_eq!(v["up_to_date"], true, "{v}");
    let (v, _) = result(&replies[4]);
    assert!(v["text"].as_str().unwrap().contains("Signing in"));
    let (v, _) = result(&replies[5]);
    assert!(v["text"].as_str().unwrap().contains("mfa admin"));
    let (v, _) = result(&replies[6]);
    assert!(v["mermaid"].as_str().unwrap().starts_with("flowchart LR"));
    let (v, err) = result(&replies[7]);
    assert!(
        err && has_code(&v, "SVT001"),
        "an invalid file gets its diagnostics: {v}"
    );
    let (v, err) = result(&replies[9]);
    assert!(
        !err && v["mermaid"].as_str().unwrap().contains("Order") && has_code(&v, "SVT001"),
        "a complete but unsafe design is still drawn, with its errors: {v}"
    );
    let (v, err) = result(&replies[8]);
    assert!(
        err && v["error"].as_str().unwrap().contains("svetra init"),
        "{v}"
    );
}

#[test]
fn protocol_errors() {
    let dir = project("errors", include_str!("fixtures/clinic.yaml"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_svetra"))
        .arg("mcp")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "not json").unwrap();
    writeln!(
        stdin,
        "{}",
        json!({ "jsonrpc": "2.0", "id": 1, "method": "resources/list" })
    )
    .unwrap();
    writeln!(stdin, "{}", call(2, "deploy", json!({}))).unwrap();
    drop(stdin);
    let out = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap();
    let codes: Vec<i64> = out
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["error"]["code"]
                .as_i64()
                .unwrap()
        })
        .collect();
    assert_eq!(codes, [-32700, -32601, -32602]);
}

/// An agent calling `compile` with a hole body that is a whole function gets the problem against
/// the body file, as a tool error it can act on, and nothing is written.
#[test]
fn compile_refuses_a_hole_body_that_is_a_whole_function() {
    let dir = project("body", include_str!("fixtures/finledger.yaml"));
    std::fs::create_dir_all(dir.join("supabase/holes")).unwrap();
    std::fs::write(
        dir.join("supabase/holes/transfer.sql"),
        "begin\n  perform 1;\nend\n",
    )
    .unwrap();
    let replies = session(&dir, &[call(1, "compile", json!({}))]);
    let (v, is_error) = result(&replies[0]);
    assert!(is_error);
    let body = &v["hole_bodies"][0];
    assert_eq!(body["file"], "supabase/holes/transfer.sql");
    assert_eq!(body["diagnostics"][0]["code"], "SVT032");
    assert_eq!(
        body["diagnostics"][0]["remediation"]["action"],
        "fix_hole_body"
    );
    assert!(!dir.join("supabase/schemas/svetra.sql").exists());
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/check/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// The `check` tool on a saved snapshot gives the CLI's `--json` document and a summary.
#[test]
fn check_reads_a_snapshot_and_matches_the_cli_json() {
    let dir = project("check-snapshot", include_str!("fixtures/clinic.yaml"));
    let replies = session(
        &dir,
        &[call(
            1,
            "check",
            json!({ "snapshot": fixture("leaky_app.json") }),
        )],
    );
    let (mut got, is_error) = result(&replies[0]);
    assert!(!is_error, "findings are results, not errors: {got}");
    let summary = got.as_object_mut().unwrap().remove("summary").unwrap();
    let summary = summary.as_str().unwrap();
    assert!(
        summary.contains("critical") && summary.contains("in public"),
        "{summary}"
    );
    let golden: Value = serde_json::from_str(include_str!("golden/leaky_app.check.json")).unwrap();
    assert_eq!(got, golden);
}

#[test]
fn check_usage_errors_are_tool_errors() {
    let dir = project("check-usage", include_str!("fixtures/clinic.yaml"));
    let snap = fixture("leaky_app.json");
    let replies = session(
        &dir,
        &[
            call(1, "check", json!({ "snapshot": snap, "local": true })),
            call(2, "check", json!({})),
            call(3, "check", json!({ "snapshot": snap, "strict": true })),
            call(4, "check", json!({ "snapshot": "missing.json" })),
            call(5, "check", json!({ "snapshot": snap, "schemas": [] })),
        ],
    );
    for reply in &replies {
        let (v, is_error) = result(reply);
        assert!(is_error, "{v}");
        assert!(v["error"].is_string(), "{v}");
    }
}

/// A connection string's password never comes back, whatever happens to the connection.
#[test]
fn check_never_echoes_a_password() {
    let dir = project("check-redact", include_str!("fixtures/clinic.yaml"));
    let replies = session(
        &dir,
        &[call(
            1,
            "check",
            json!({ "db": "postgresql://app:hunter2secret@127.0.0.1:1/postgres" }),
        )],
    );
    let text = replies[0].to_string();
    assert!(!text.contains("hunter2secret"), "{text}");
}

/// Needs Docker, like tests/check_migrations.rs.
#[test]
fn check_builds_migrations_and_reports_what_applied() {
    if !svetra::migrations::docker_available() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let dir = project("check-migrations", include_str!("fixtures/clinic.yaml"));
    let replies = session(
        &dir,
        &[
            call(1, "check", json!({ "migrations": fixture("migrations") })),
            call(
                2,
                "check",
                json!({ "migrations": fixture("migrations_broken"), "strict": true }),
            ),
        ],
    );
    let (mut got, is_error) = result(&replies[0]);
    assert!(!is_error, "{got}");
    got.as_object_mut().unwrap().remove("summary").unwrap();
    let migrations = got.as_object_mut().unwrap().remove("migrations").unwrap();
    assert_eq!(
        migrations,
        json!({ "files": 3, "applied": 3, "failed": [] })
    );
    let golden: Value = serde_json::from_str(include_str!("golden/leaky_app.check.json")).unwrap();
    assert_eq!(got, golden);
    let (strict, is_error) = result(&replies[1]);
    assert!(
        is_error,
        "strict makes a failed migration an error: {strict}"
    );
    assert_eq!(strict["migrations"]["failed"].as_array().unwrap().len(), 1);
}

#[test]
fn doctor_reads_a_folder_and_matches_the_cli_json_plus_a_summary() {
    let dir = project("doctor-folder", include_str!("fixtures/clinic.yaml"));
    let app = std::env::temp_dir().join(format!("svetra-mcp-doctor-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app);
    std::fs::create_dir_all(app.join("supabase")).unwrap();
    std::fs::write(
        app.join("supabase/config.toml"),
        "[auth]\nminimum_password_length = 6\n",
    )
    .unwrap();
    let replies = session(&dir, &[call(1, "doctor", json!({ "dir": app }))]);
    let (mut got, is_error) = result(&replies[0]);
    assert!(!is_error, "{got}");
    let summary = got.as_object_mut().unwrap().remove("summary").unwrap();
    assert!(
        summary.as_str().unwrap().contains("Not a clearance"),
        "{summary}"
    );
    let cli = Command::new(env!("CARGO_BIN_EXE_svetra"))
        .args(["doctor", app.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    let cli: Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(got, cli);
    assert_eq!(got["ready"], false);
    let kinds: Vec<&str> = got["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"weak_password"), "{got}");
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn doctor_usage_errors_are_tool_errors() {
    let dir = project("doctor-usage", include_str!("fixtures/clinic.yaml"));
    let replies = session(
        &dir,
        &[
            call(
                1,
                "doctor",
                json!({ "db": "postgresql://x@127.0.0.1:1/p", "local": true }),
            ),
            call(2, "doctor", json!({ "strict": true })),
            call(3, "doctor", json!({ "schemas": [] })),
            call(4, "doctor", json!({ "dir": "no-such-folder" })),
        ],
    );
    for reply in &replies {
        let (v, is_error) = result(reply);
        assert!(is_error, "{v}");
        assert!(v["error"].is_string(), "{v}");
    }
}

#[test]
fn doctor_never_echoes_a_password() {
    let dir = project("doctor-redact", include_str!("fixtures/clinic.yaml"));
    let replies = session(
        &dir,
        &[call(
            1,
            "doctor",
            json!({ "db": "postgresql://app:hunter2secret@127.0.0.1:1/postgres" }),
        )],
    );
    let text = replies[0].to_string();
    assert!(!text.contains("hunter2secret"), "{text}");
}

/// Needs Docker, like tests/check_migrations.rs.
#[test]
fn doctor_builds_migrations_and_reports_data_findings_with_a_summary() {
    if !svetra::migrations::docker_available() {
        eprintln!("skipped: Docker isn't running");
        return;
    }
    let dir = project("doctor-migrations", include_str!("fixtures/clinic.yaml"));
    let replies = session(
        &dir,
        &[call(
            1,
            "doctor",
            json!({ "migrations": fixture("migrations") }),
        )],
    );
    let (got, is_error) = result(&replies[0]);
    assert!(!is_error, "{got}");
    assert!(got["counts"]["critical"].as_u64().unwrap() > 0, "{got}");
    assert!(
        got["summary"].as_str().unwrap().contains("Not ready"),
        "{got}"
    );
    assert_eq!(got["migrations"]["applied"], 3);
    assert!(
        got["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["area"] == "data"),
        "{got}"
    );
}

#[test]
fn patch_applies_edits_and_refusals_and_conflicts_are_results() {
    let yaml = include_str!("fixtures/finledger.yaml");
    let dir = project("patch", yaml);
    let replies = session(
        &dir,
        &[
            call(1, "context", json!({})),
            call(
                2,
                "patch",
                json!({ "ops": [{ "op": "set", "path": "entities.Account.fields.nickname", "value": { "type": "text" } }],
                        "expected_revision": "000000000000" }),
            ),
            call(
                3,
                "patch",
                json!({ "ops": [{ "op": "remove", "path": "entities.Nope" }] }),
            ),
            call(
                4,
                "patch",
                json!({ "ops": [{ "op": "set", "path": "entities.Account.fields.nickname", "value": { "type": "text" } }] }),
            ),
            call(5, "patch", json!({ "ops": "nope" })),
            call(6, "patch", json!({})),
        ],
    );
    let (context, _) = result(&replies[0]);
    assert_eq!(
        context["revision"],
        svetra::compile::revision(yaml),
        "context carries the revision"
    );
    let (conflict, is_error) = result(&replies[1]);
    assert!(!is_error, "a conflict is a result");
    assert_eq!(conflict["conflict"], true);
    let (refused, is_error) = result(&replies[2]);
    assert!(!is_error, "a refusal is a result");
    assert_eq!(refused["applied"], false);
    assert_eq!(refused["problems"][0]["at"], "ops[0]");
    let (applied, is_error) = result(&replies[3]);
    assert!(!is_error);
    assert_eq!(applied["applied"], true);
    assert!(
        std::fs::read_to_string(dir.join("svetra.yaml"))
            .unwrap()
            .contains("nickname")
    );
    assert!(
        result(&replies[4]).1 && result(&replies[5]).1,
        "bad input is a tool error"
    );
}
