//! `svetra mcp`: an MCP server over stdio (docs/agents.md). JSON-RPC 2.0, one message per
//! line. Tools validate, compile and describe a `svetra.yaml`, and return JSON text.

use crate::project::{self, Failure};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;

/// MCP protocol versions this server speaks. The first is offered when the client asks for
/// one it doesn't know.
const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Svetra checks and compiles svetra.yaml, the design of a Supabase app: its data, who can see and change what, sign-in settings, and holes for custom code. After every edit to svetra.yaml, call validate and fix each error using its remediation hint. Then call compile. Write custom code only in supabase/holes/<name>.sql. Use explain to tell the person what the app allows, in plain words.";

/// Run the server until stdin closes.
pub fn serve() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle(&line) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// One incoming line to at most one reply. Notifications (no `id`) get none.
pub fn handle(line: &str) -> Option<String> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Some(error(Value::Null, -32700, &format!("Parse error: {e}"))),
    };
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = asked
                .filter(|v| VERSIONS.contains(v))
                .unwrap_or(VERSIONS[0]);
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "svetra", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call(name, &args) {
                Some(r) => r,
                None => return Some(error(id, -32602, &format!("Unknown tool: {name}"))),
            }
        }
        _ => return Some(error(id, -32601, &format!("Method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string())
}

fn error(id: Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

fn tools() -> Value {
    let file = json!({
        "type": "string",
        "description": "Path to svetra.yaml, relative to the server's working directory. Default: svetra.yaml"
    });
    let only_file =
        json!({ "type": "object", "properties": { "file": file }, "additionalProperties": false });
    json!([
        { "name": "validate", "description": "Check svetra.yaml. Returns every problem with a code, position and remediation hint, as JSON. Call it after every edit.", "inputSchema": only_file },
        { "name": "compile", "description": "Write the Postgres schema, row-level security, TypeScript and Zod types, access tests and sign-in settings from svetra.yaml. With check: true, writes nothing and lists what is out of date.", "inputSchema": {
            "type": "object",
            "properties": { "file": file, "check": { "type": "boolean", "description": "Only check whether generated files are current" } },
            "additionalProperties": false
        } },
        { "name": "explain", "description": "The app in plain English: what it stores, who can see and change what, sign-in, custom code. Use it to answer the person's questions.", "inputSchema": only_file },
        { "name": "context", "description": "The whole design in a few lines, for reading it compactly instead of the generated files.", "inputSchema": only_file },
        { "name": "graph", "description": "The design as a Mermaid diagram.", "inputSchema": only_file },
    ])
}

/// A tool result: JSON text, with `isError` when the tool couldn't do its job.
fn text(value: Value, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": value.to_string() }], "isError": is_error })
}

fn diagnostics(file: &str, diags: &[crate::diag::Diagnostic]) -> Value {
    serde_json::from_str(&crate::report::json(file, diags)).unwrap_or(Value::Null)
}

fn call(name: &str, args: &Value) -> Option<Value> {
    let file = args
        .get("file")
        .and_then(Value::as_str)
        .unwrap_or("svetra.yaml");
    let unreadable = |e: String| {
        text(
            json!({ "error": format!("{e}. Pass `file`, or run `svetra init` to start one.") }),
            true,
        )
    };
    Some(match name {
        "validate" => match project::read(Path::new(file)) {
            Ok(src) => text(diagnostics(file, &crate::report::validate(&src)), false),
            Err(Failure::Read(e)) => unreadable(e),
            Err(_) => unreachable!("read only fails to read"),
        },
        "compile" => {
            let check = args.get("check").and_then(Value::as_bool).unwrap_or(false);
            match project::compile(Path::new(file), check) {
                Ok(run) => text(
                    json!({
                        "written": run.written,
                        "stubs": run.stubs,
                        "config_updated": run.config_updated,
                        "no_config": run.no_config,
                        "stale": run.stale,
                        "up_to_date": check && run.stale.is_empty(),
                        "notes": diagnostics(file, &run.notes)["diagnostics"],
                    }),
                    false,
                ),
                Err(Failure::Read(e)) => unreadable(e),
                Err(Failure::Invalid(d)) => text(diagnostics(file, &d), true),
                Err(Failure::Write(e)) => text(json!({ "error": e }), true),
            }
        }
        "explain" | "context" | "graph" => {
            let src = match project::read(Path::new(file)) {
                Ok(s) => s,
                Err(Failure::Read(e)) => return Some(unreadable(e)),
                Err(_) => unreachable!("read only fails to read"),
            };
            let diags = crate::report::validate(&src);
            if crate::report::has_errors(&diags) {
                return Some(text(diagnostics(file, &diags), true));
            }
            let (doc, _) = crate::load::load(&src);
            match name {
                "explain" => text(
                    json!({ "text": crate::understand::explain::explain(&doc, &diags) }),
                    false,
                ),
                "context" => text(
                    json!({ "text": crate::understand::context::context(&doc) }),
                    false,
                ),
                _ => text(
                    json!({ "mermaid": crate::understand::graph::graph(&doc) }),
                    false,
                ),
            }
        }
        _ => return None,
    })
}
