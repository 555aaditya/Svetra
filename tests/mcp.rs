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
fn handshake_lists_the_five_tools() {
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
        ["validate", "compile", "explain", "context", "graph"]
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
