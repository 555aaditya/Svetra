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
        { "name": "plan", "description": "What a change to svetra.yaml does to the running app, before it's applied: every change classified as safe, breaking or destructive for the data, and whose access widens or narrows, compared with the committed design (git HEAD) or `from`. Read-only. Show every destructive and wider line to the person in their words; only after their explicit yes, run `svetra plan --accept <kinds>` to write any rename migration.", "inputSchema": {
            "type": "object",
            "properties": { "file": file, "from": { "type": "string", "description": "Path to the design the database has. Default: file as committed in git HEAD" } },
            "additionalProperties": false
        } },
        { "name": "check", "description": "What is open in an existing Supabase database or in its migrations: tables without row-level security, policies open to everyone, views that skip it, risky functions. Needs no svetra.yaml and never writes. Give exactly one of db, local, snapshot or migrations. Returns findings with a fix each, counts, and a summary. Findings are results, not errors: read counts.", "inputSchema": {
            "type": "object",
            "properties": {
                "db": { "type": "string", "description": "Connection string, read through psql" },
                "local": { "type": "boolean", "description": "true: the database `supabase start` runs" },
                "snapshot": { "type": "string", "description": "Path to a saved snapshot JSON (from `svetra check --print-query`)" },
                "migrations": { "description": "A folder of migrations, or true for supabase/migrations. Built in a throwaway database (Docker, or `scratch`)", "oneOf": [{ "type": "string" }, { "type": "boolean" }] },
                "scratch": { "type": "string", "description": "With migrations: an empty database to build them in, instead of Docker" },
                "strict": { "type": "boolean", "description": "With migrations: a migration that fails to apply is an error" },
                "schemas": { "type": "array", "items": { "type": "string" }, "description": "Schemas the API exposes. Default: [\"public\"]" }
            },
            "additionalProperties": false
        } },
        { "name": "doctor", "description": "The pre-launch checklist for a Supabase project folder: what is open in the database (check's findings), secret keys in code, a committed .env, risky sign-in settings, public storage buckets, generated files that drifted, and what to back up. Needs no svetra.yaml and never writes. Give at most one database source: db, local or migrations; without one the database is listed under not_checked. Run it before telling anyone an app is ready. Findings are results, not errors: read counts and ready (true only for a full, clean run).", "inputSchema": {
            "type": "object",
            "properties": {
                "dir": { "type": "string", "description": "The project folder. Default: the server's working directory" },
                "db": { "type": "string", "description": "Connection string, read through psql" },
                "local": { "type": "boolean", "description": "true: the database `supabase start` runs" },
                "migrations": { "description": "A folder of migrations, or true for dir/supabase/migrations. Built in a throwaway database (Docker, or `scratch`)", "oneOf": [{ "type": "string" }, { "type": "boolean" }] },
                "scratch": { "type": "string", "description": "With migrations: an empty database to build them in, instead of Docker" },
                "strict": { "type": "boolean", "description": "With migrations: a migration that fails to apply is an error" },
                "schemas": { "type": "array", "items": { "type": "string" }, "description": "Schemas the API exposes. Default: [\"public\"]" }
            },
            "additionalProperties": false
        } },
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
                Err(Failure::HoleBodies(bodies)) => text(
                    json!({ "hole_bodies": bodies.iter().map(|(path, d)| diagnostics(path, std::slice::from_ref(d))).collect::<Vec<_>>() }),
                    true,
                ),
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
            let errors = crate::report::has_errors(&diags);
            if errors && (name == "explain" || !crate::report::drawable(&diags)) {
                return Some(text(diagnostics(file, &diags), true));
            }
            let (doc, _) = crate::load::load(&src);
            let mut out = match name {
                "explain" => json!({ "text": crate::understand::explain::explain(&doc, &diags) }),
                "context" => json!({ "text": crate::understand::context::context(&doc) }),
                _ => json!({ "mermaid": crate::understand::graph::graph(&doc) }),
            };
            // Drawn despite errors: say so, with the errors to fix.
            if errors {
                out["diagnostics"] = diagnostics(file, &diags)["diagnostics"].clone();
            }
            text(out, false)
        }
        "plan" => {
            let src = match project::read(Path::new(file)) {
                Ok(s) => s,
                Err(Failure::Read(e)) => return Some(unreadable(e)),
                Err(_) => unreachable!("read only fails to read"),
            };
            let old = match args.get("from").and_then(Value::as_str) {
                Some(f) => project::read(Path::new(f))
                    .map(|s| (s, f.to_string()))
                    .map_err(|_| format!("cannot read {f}")),
                None => crate::change::committed(Path::new(file)),
            };
            let (old_src, from) = match old {
                Ok(v) => v,
                Err(e) => return Some(text(json!({ "error": e }), true)),
            };
            for (name, s) in [(from.as_str(), &old_src), (file, &src)] {
                let d = crate::report::validate(s);
                if crate::report::has_errors(&d) {
                    return Some(text(diagnostics(name, &d), true));
                }
            }
            let (old_doc, _) = crate::load::load(&old_src);
            let (new_doc, _) = crate::load::load(&src);
            let plan = crate::change::plan(&old_doc, &old_src, &new_doc, &src);
            if crate::report::has_errors(&plan.diagnostics) {
                return Some(text(diagnostics(file, &plan.diagnostics), true));
            }
            let mut out = crate::change::json(&plan, &from, &[]);
            if !plan.diagnostics.is_empty() {
                out["diagnostics"] = diagnostics(file, &plan.diagnostics)["diagnostics"].clone();
            }
            text(out, false)
        }
        "check" => check(args),
        "doctor" => doctor(args),
        _ => return None,
    })
}

/// The `check` tool (docs/agents.md): the CLI's `--json` document plus a `summary`.
fn check(args: &Value) -> Value {
    let err = |m: &str| text(json!({ "error": m }), true);
    let string = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
    let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
    let migrations = match args.get("migrations") {
        Some(Value::String(d)) if !d.is_empty() => Some(d.clone()),
        Some(Value::Bool(true)) => Some("supabase/migrations".to_string()),
        _ => None,
    };
    let (local, scratch, strict) = (flag("local"), string("scratch"), flag("strict"));
    let db = if local {
        Some("postgresql://postgres:postgres@127.0.0.1:54322/postgres".to_string())
    } else {
        string("db")
    };
    let snapshot = string("snapshot");
    let given = [
        db.is_some() || args.get("db").is_some(),
        snapshot.is_some(),
        migrations.is_some(),
    ];
    if given.iter().filter(|g| **g).count() != 1 || (local && args.get("db").is_some()) {
        return err(
            "Pass exactly one of db, local, snapshot or migrations (for example migrations: true).",
        );
    }
    if migrations.is_none() && (scratch.is_some() || strict) {
        return err("scratch and strict go with migrations.");
    }
    let schemas = match schemas(args) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let source = match (migrations, db, snapshot) {
        (Some(dir), _, _) => crate::inspect::Source::Migrations { dir, scratch },
        (None, Some(url), _) => crate::inspect::Source::Db(url),
        (None, None, Some(f)) => crate::inspect::Source::Snapshot(f),
        _ => return err("db must be a connection string."),
    };
    let report = match crate::inspect::run(&source, &schemas) {
        Ok(r) => r,
        Err(crate::inspect::CheckError::Usage(m) | crate::inspect::CheckError::Fail(m)) => {
            return err(&m);
        }
    };
    let mut out = report.json();
    if let Some(o) = report
        .migrations
        .as_ref()
        .filter(|o| strict && !o.failed.is_empty())
    {
        out["error"] = json!(format!(
            "{} of {} migrations could not be applied. Fix them, or drop strict to accept partial findings.",
            o.failed.len(),
            o.files
        ));
        return text(out, true);
    }
    out["summary"] = json!(summary(&report, &out["counts"], &schemas));
    text(out, false)
}

/// The `schemas` input, default `["public"]`; a tool error when it names none.
fn schemas(args: &Value) -> Result<Vec<String>, Value> {
    let schemas: Vec<String> = match args.get("schemas").and_then(Value::as_array) {
        Some(a) => a
            .iter()
            .filter_map(|s| s.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        None => vec!["public".to_string()],
    };
    if schemas.is_empty() {
        return Err(text(
            json!({ "error": "schemas must name at least one schema, such as [\"public\"]." }),
            true,
        ));
    }
    Ok(schemas)
}

/// The `doctor` tool (docs/agents.md): the CLI's `--json` document plus a `summary`.
fn doctor(args: &Value) -> Value {
    let err = |m: &str| text(json!({ "error": m }), true);
    let string = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
    let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
    let dir = string("dir").unwrap_or_else(|| ".".to_string());
    let migrations = match args.get("migrations") {
        Some(Value::String(d)) if !d.is_empty() => Some(d.clone()),
        Some(Value::Bool(true)) => Some(format!("{dir}/supabase/migrations")),
        _ => None,
    };
    let (local, scratch, strict) = (flag("local"), string("scratch"), flag("strict"));
    let db = string("db");
    let given = [db.is_some(), local, migrations.is_some()];
    if given.iter().filter(|g| **g).count() > 1 {
        return err("Pass at most one of db, local and migrations.");
    }
    if migrations.is_none() && (scratch.is_some() || strict) {
        return err("scratch and strict go with migrations.");
    }
    let schemas = match schemas(args) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let source = match (migrations, local, db) {
        (Some(dir), _, _) => Some(crate::inspect::Source::Migrations { dir, scratch }),
        (None, true, _) => Some(crate::inspect::Source::Db(
            "postgresql://postgres:postgres@127.0.0.1:54322/postgres".to_string(),
        )),
        (None, false, Some(url)) => Some(crate::inspect::Source::Db(url)),
        _ => None,
    };
    let report = match crate::doctor::run(Path::new(&dir), source.as_ref(), &schemas) {
        Ok(r) => r,
        Err(crate::inspect::CheckError::Usage(m) | crate::inspect::CheckError::Fail(m)) => {
            return err(&m);
        }
    };
    let mut out: Value = serde_json::from_str(&report.json()).unwrap_or(Value::Null);
    if let Some(o) = report
        .migrations
        .as_ref()
        .filter(|o| strict && !o.failed.is_empty())
    {
        out["error"] = json!(format!(
            "{} of {} migrations could not be applied. Fix them, or drop strict to accept partial findings.",
            o.failed.len(),
            o.files
        ));
        return text(out, true);
    }
    out["summary"] = json!(doctor_summary(&report));
    text(out, false)
}

/// One plain-English sentence for `doctor`: the counts and the verdict. Names no database.
fn doctor_summary(r: &crate::doctor::Report) -> String {
    use crate::doctor::Severity;
    let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    let c = r.count(Severity::Critical);
    let mut out = format!(
        "{c} critical, {}, {}.",
        plural(r.count(Severity::Warning), "warning"),
        plural(r.count(Severity::Info), "note")
    );
    out += &if c > 0 {
        format!(
            " Fix the {} first. Not ready.",
            plural(c, "critical finding")
        )
    } else if r.ready() {
        " No critical findings in everything doctor can check; the reminders are yours.".to_string()
    } else {
        let n = r.not_checked.len();
        format!(
            " No critical findings, but {n} {} not checked. Not a clearance.",
            if n == 1 { "part was" } else { "parts were" }
        )
    };
    out
}

/// One plain-English sentence with the counts; the source is redacted.
fn summary(report: &crate::inspect::Report, counts: &Value, schemas: &[String]) -> String {
    let n = |k: &str| counts[k].as_u64().unwrap_or(0);
    let plural = |n: u64, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    let mut out = match &report.migrations {
        Some(o) if o.applied == 0 => format!(
            "None of the {} migrations in {} could be applied, so nothing was checked.",
            o.files, report.source
        ),
        _ => format!(
            "{} critical, {}, {} in {} ({}).",
            n("critical"),
            plural(n("warning"), "warning"),
            plural(n("info"), "note"),
            schemas.join(", "),
            report.source
        ),
    };
    if let Some(o) = report.migrations.as_ref().filter(|o| o.applied > 0) {
        out += &format!(" {} of {} migrations applied", o.applied, o.files);
        out += if o.failed.is_empty() {
            "."
        } else {
            "; the failed ones are listed, so the findings may be incomplete."
        };
        out += " This reads the migrations, not the live database.";
    }
    out
}
