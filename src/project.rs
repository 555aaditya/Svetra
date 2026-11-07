//! A project on disk: `svetra.yaml` and the files `compile` reads and writes next to it.
//! Shared by the CLI and the MCP server, so both behave the same.

use crate::diag::Diagnostic;
use std::collections::HashMap;
use std::path::Path;

/// Supabase's own config file, where `compile` sets only the sign-in keys it owns.
pub const CONFIG: &str = "supabase/config.toml";

/// What a `compile` run did, or with `check`, what it found out of date.
#[derive(Debug, Default)]
pub struct Compiled {
    /// Hole bodies written as stubs because they didn't exist yet.
    pub stubs: Vec<String>,
    /// Generated files written.
    pub written: Vec<String>,
    /// The sign-in settings in `supabase/config.toml` were changed.
    pub config_updated: bool,
    /// The file has an `auth` section, but there's no `supabase/config.toml` to apply it to.
    pub no_config: bool,
    /// Warnings and info from validation.
    pub notes: Vec<Diagnostic>,
    /// With `check`: each out-of-date file, or `supabase/config.toml (<key> is …)`.
    pub stale: Vec<String>,
}

#[derive(Debug)]
pub enum Failure {
    /// The file couldn't be read: a usage problem, not a design problem.
    Read(String),
    /// Validation errors: nothing was written.
    Invalid(Vec<Diagnostic>),
    /// A file couldn't be written.
    Write(String),
}

/// Write to a temporary file in the same folder, then rename, so a crash never leaves half a file.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("svetra-tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

pub fn read(file: &Path) -> Result<String, Failure> {
    std::fs::read_to_string(file)
        .map_err(|e| Failure::Read(format!("cannot read {}: {e}", file.display())))
}

/// `svetra compile [FILE] [--check]` (docs/compiler.md).
pub fn compile(file: &Path, check: bool) -> Result<Compiled, Failure> {
    let src = read(file)?;
    let name = file.file_name().map_or_else(
        || file.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let dir = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut out = Compiled::default();
    let diags = crate::report::validate(&src);
    if crate::report::has_errors(&diags) {
        return Err(Failure::Invalid(diags));
    }
    // Hole bodies are yours: read each one, and write a stub only where there's none yet.
    let (doc, _) = crate::load::load(&src);
    let mut bodies = HashMap::new();
    for h in &doc.holes {
        let rel = format!("supabase/holes/{}.sql", h.name);
        match std::fs::read_to_string(dir.join(&rel)) {
            Ok(body) => {
                bodies.insert(h.name.clone(), body);
            }
            Err(_) if !check => {
                write_atomic(&dir.join(&rel), &crate::compile::sql::stub(&h.name))
                    .map_err(|e| Failure::Write(format!("cannot write {rel}: {e}")))?;
                out.stubs.push(rel);
            }
            Err(_) => {}
        }
    }
    let (outputs, notes) =
        crate::compile::compile_with(&src, &name, &bodies).map_err(Failure::Invalid)?;
    out.notes = notes;
    // Sign-in settings go into Supabase's own config.toml, which Svetra edits but doesn't own.
    let config_path = dir.join(CONFIG);
    let config = match (&doc.auth, std::fs::read_to_string(&config_path)) {
        (None, _) => None,
        (Some(_), Ok(existing)) => {
            let settings = crate::compile::config::settings(&doc, &existing);
            Some((existing, settings))
        }
        (Some(_), Err(_)) => {
            out.no_config = true;
            None
        }
    };
    if check {
        out.stale = outputs
            .iter()
            .filter(|o| {
                std::fs::read_to_string(dir.join(o.path)).ok().as_deref()
                    != Some(o.contents.as_str())
            })
            .map(|o| o.path.to_string())
            .collect();
        if let Some((existing, settings)) = &config {
            for d in crate::compile::config::differences(existing, settings) {
                out.stale.push(format!("{CONFIG} ({d})"));
            }
        }
        return Ok(out);
    }
    for o in &outputs {
        write_atomic(&dir.join(o.path), &o.contents)
            .map_err(|e| Failure::Write(format!("cannot write {}: {e}", o.path)))?;
        out.written.push(o.path.to_string());
    }
    if let Some((existing, settings)) = config {
        let updated = crate::compile::config::apply(&existing, &settings);
        if updated != existing {
            write_atomic(&config_path, &updated)
                .map_err(|e| Failure::Write(format!("cannot write {CONFIG}: {e}")))?;
            out.config_updated = true;
        }
    }
    Ok(out)
}

/// The Svetra skill for Claude Code (docs/agents.md, The skill).
pub const SKILL: &str = include_str!("skill.md");

/// The `svetra.yaml` that `init` writes when there's none: one entity people own, its rule,
/// and safe sign-in settings.
pub const STARTER: &str = r#"# The design of your app. Your agent edits it with you; `svetra validate` checks every change.
# yaml-language-server: $schema=https://raw.githubusercontent.com/555aaditya/Svetra/main/schema/svetra.schema.json
svetra: "0.1"
system: App

auth:
  sign_in: [email]

entities:
  Note:
    fields:
      id:     { type: uuid, primary: true }
      author: { type: user, required: true }
      body:   { type: text, required: true }
    indexes:
      - [author]

policies:
  own_notes:
    on: Note
    allow: [select, insert, update, delete]
    when: "author == auth.user"
"#;

/// What `init` did, one line per file.
pub struct Initialised {
    pub lines: Vec<String>,
}

/// `svetra init [DIR]` (docs/agents.md): the skill, the MCP server entry, and a starter file.
pub fn init(dir: &Path) -> Result<Initialised, Failure> {
    let mut lines = Vec::new();
    let write = |rel: &str, contents: &str| {
        write_atomic(&dir.join(rel), contents)
            .map_err(|e| Failure::Write(format!("cannot write {rel}: {e}")))
    };
    write(".claude/skills/svetra/SKILL.md", SKILL)?;
    lines.push("wrote .claude/skills/svetra/SKILL.md (the Svetra skill)".to_string());

    // .mcp.json may list other servers: keep them, and add or update only `svetra`.
    let server = serde_json::json!({ "command": "npx", "args": ["-y", "svetra", "mcp"] });
    let path = dir.join(".mcp.json");
    let mut config = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<serde_json::Value>(&text).map_err(|e| {
            Failure::Write(format!(
                ".mcp.json isn't valid JSON ({e}); fix it, or add the svetra server to it by hand"
            ))
        })?,
        Err(_) => serde_json::json!({}),
    };
    let Some(servers) = config
        .as_object_mut()
        .map(|o| {
            o.entry("mcpServers")
                .or_insert_with(|| serde_json::json!({}))
        })
        .and_then(|s| s.as_object_mut())
    else {
        return Err(Failure::Write(".mcp.json doesn't hold an object with `mcpServers`; add the svetra server to it by hand".into()));
    };
    let existed = servers.insert("svetra".into(), server).is_some();
    let pretty =
        serde_json::to_string_pretty(&config).map_err(|e| Failure::Write(e.to_string()))?;
    write(".mcp.json", &format!("{pretty}\n"))?;
    lines.push(
        if existed {
            "updated the svetra server in .mcp.json"
        } else {
            "added the svetra server to .mcp.json"
        }
        .to_string(),
    );

    if dir.join("svetra.yaml").exists() {
        lines.push("kept svetra.yaml (it's yours)".to_string());
    } else {
        write("svetra.yaml", STARTER)?;
        lines.push(
            "wrote svetra.yaml (a starting point: notes that only their author can see)"
                .to_string(),
        );
    }
    Ok(Initialised { lines })
}
