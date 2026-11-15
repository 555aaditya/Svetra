use std::process::ExitCode;
use svetra::report;

const HELP: &str = "svetra: design the system with the AI, compile it to the low level

Usage:
  svetra validate [FILE] [--json]   Check a svetra.yaml file (default: ./svetra.yaml)
  svetra explain [FILE]             Describe the app in plain English: data, rules, who can see what
  svetra graph [FILE]               Draw the app as a Mermaid diagram
  svetra context [FILE]             Print the whole app compactly, for agents
  svetra diff [FILE] [--from OLD] [--json]
                                    List the changes since OLD (default: FILE in git HEAD),
                                    classified: safe, breaking or destructive, and whose
                                    access widens or narrows
  svetra plan [FILE] [--from OLD] [--json] [--accept KIND,...] [--at YYYYMMDDHHMMSS]
                                    The same, as a plan: exits 1 on destructive changes or
                                    wider access until they're accepted, then writes the
                                    migration for any declared renames
  svetra view [FILE] [--out PATH] [--no-open] [--json]
                                    Write one page showing who can see and change what
                                    (default .svetra/view.html) and open it; --json prints
                                    the view model instead
  svetra compile [FILE] [--check]   Write the SQL schema, TypeScript types and access tests next
                                    to FILE, and sign-in settings into supabase/config.toml;
                                    --check writes nothing and fails if they are out of date
  svetra init [DIR]                 Set up a project for agents: the Svetra skill, the MCP
                                    server in .mcp.json, and a starter svetra.yaml
  svetra mcp                        Serve validate, compile, explain, context and graph to
                                    agents over MCP (stdio)
  svetra --version                  Print the version
  svetra --help                     Print this help

Exit codes: 0 ok, 1 errors found (or stale files with --check), 2 usage error.
";

// ponytail: hand-rolled argument parsing; each command takes one file and one flag. Switch to
// clap when commands grow real options.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V") => {
            println!("svetra {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        None | Some("--help" | "-h") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some("validate") => validate(&args[1..]),
        Some("compile") => compile(&args[1..]),
        Some("init") => init(&args[1..]),
        Some("explain") => view("explain", &args[1..], false, |doc, diags| {
            svetra::understand::explain::explain(doc, diags)
        }),
        Some("graph") => view("graph", &args[1..], true, |doc, _| {
            svetra::understand::graph::graph(doc)
        }),
        Some("context") => view("context", &args[1..], true, |doc, _| {
            svetra::understand::context::context(doc)
        }),
        Some("view") => page(&args[1..]),
        Some(cmd @ ("diff" | "plan")) => change(cmd, &args[1..]),
        Some("mcp") if args.len() == 1 => match svetra::mcp::serve() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("svetra: {e}");
                ExitCode::from(1)
            }
        },
        Some(other) => usage(&format!("unknown argument '{other}'")),
    }
}

/// Parse `[FILE] [flag]`. Returns the file (default `svetra.yaml`) and whether `flag` was given.
fn file_and_flag<'a>(
    command: &str,
    args: &'a [String],
    flag: &str,
) -> Result<(&'a str, bool), String> {
    let set = args.iter().any(|a| a == flag);
    let files: Vec<&String> = args.iter().filter(|a| *a != flag).collect();
    if let Some(bad) = files.iter().find(|a| a.starts_with('-')) {
        return Err(format!("unknown option '{bad}'"));
    }
    if files.len() > 1 {
        return Err(format!("{command} takes one file"));
    }
    Ok((files.first().map_or("svetra.yaml", |f| f.as_str()), set))
}

fn read(file: &str) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))
}

fn validate(args: &[String]) -> ExitCode {
    let (file, json) = match file_and_flag("validate", args, "--json") {
        Ok(v) => v,
        Err(e) => return usage(&e),
    };
    let src = match read(file) {
        Ok(s) => s,
        Err(e) => return usage(&e),
    };
    let diags = report::validate(&src);
    print!(
        "{}",
        if json {
            report::json(file, &diags)
        } else {
            report::human(file, &diags)
        }
    );
    if report::has_errors(&diags) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// A read-only view of a valid file: validate first, then print `render`'s output. With
/// `drawable`, a file whose only errors leave the design complete (`report::drawable`) is still
/// rendered, with its diagnostics on stderr and exit 1.
fn view(
    command: &str,
    args: &[String],
    drawable: bool,
    render: impl Fn(&svetra::model::Document, &[svetra::diag::Diagnostic]) -> String,
) -> ExitCode {
    let (file, _) = match file_and_flag(command, args, "--no-such-flag") {
        Ok(v) => v,
        Err(e) => return usage(&e),
    };
    let src = match read(file) {
        Ok(s) => s,
        Err(e) => return usage(&e),
    };
    let diags = report::validate(&src);
    let errors = report::has_errors(&diags);
    if errors && !(drawable && report::drawable(&diags)) {
        print!("{}", report::human(file, &diags));
        return ExitCode::from(1);
    }
    if errors {
        eprint!("{}", report::human(file, &diags));
    }
    let (doc, _) = svetra::load::load(&src);
    print!("{}", render(&doc, &diags));
    if errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn compile(args: &[String]) -> ExitCode {
    use svetra::project::{CONFIG, Failure};
    let (file, check) = match file_and_flag("compile", args, "--check") {
        Ok(v) => v,
        Err(e) => return usage(&e),
    };
    let run = match svetra::project::compile(std::path::Path::new(file), check) {
        Ok(run) => run,
        Err(Failure::Read(e)) => return usage(&e),
        Err(Failure::Invalid(diags)) => {
            print!("{}", report::human(file, &diags));
            println!("Nothing was written.");
            return ExitCode::from(1);
        }
        Err(Failure::Write(e)) => {
            eprintln!("svetra: {e}");
            return ExitCode::from(1);
        }
    };
    for s in &run.stubs {
        println!("wrote {s} (a stub: write the body there)");
    }
    if !run.notes.is_empty() {
        print!("{}", report::human(file, &run.notes));
    }
    if run.no_config {
        println!(
            "note: no {CONFIG}, so the sign-in settings weren't applied. Run `supabase init`, then `svetra compile` again."
        );
    }
    if check {
        if run.stale.is_empty() {
            println!("Generated files are up to date.");
            return ExitCode::SUCCESS;
        }
        for p in &run.stale {
            println!("out of date: {p}");
        }
        println!(
            "Run `svetra compile` to regenerate. Generated files must never be edited by hand."
        );
        return ExitCode::from(1);
    }
    for w in &run.written {
        println!("wrote {w}");
    }
    if run.config_updated {
        println!("updated the sign-in settings in {CONFIG}");
    }
    ExitCode::SUCCESS
}

fn init(args: &[String]) -> ExitCode {
    let dir = match args {
        [] => ".",
        [d] if !d.starts_with('-') => d.as_str(),
        _ => return usage("init takes one folder"),
    };
    match svetra::project::init(std::path::Path::new(dir)) {
        Ok(done) => {
            for l in &done.lines {
                println!("{l}");
            }
            println!("Next: open your agent in this folder and describe the app you want.");
            ExitCode::SUCCESS
        }
        Err(svetra::project::Failure::Write(e) | svetra::project::Failure::Read(e)) => {
            eprintln!("svetra: {e}");
            ExitCode::from(1)
        }
        Err(svetra::project::Failure::Invalid(_)) => unreachable!("init doesn't validate"),
    }
}

/// `svetra view` (docs/view.md): validate, then write the page, or print the model with `--json`.
fn page(args: &[String]) -> ExitCode {
    let (mut file, mut out, mut open, mut json) = (None, None, true, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--no-open" => open = false,
            "--out" => match it.next() {
                Some(p) => out = Some(p.clone()),
                None => return usage("--out needs a path"),
            },
            f if f.starts_with('-') => return usage(&format!("unknown option '{f}'")),
            f if file.is_none() => file = Some(f.to_string()),
            _ => return usage("view takes one file"),
        }
    }
    let file = file.unwrap_or_else(|| "svetra.yaml".into());
    let src = match read(&file) {
        Ok(s) => s,
        Err(e) => return usage(&e),
    };
    let diags = report::validate(&src);
    if report::has_errors(&diags) {
        print!("{}", report::human(&file, &diags));
        return ExitCode::from(1);
    }
    let (doc, _) = svetra::load::load(&src);
    if json {
        print!("{}", svetra::understand::view::json(&doc, &src));
        return ExitCode::SUCCESS;
    }
    let dir = std::path::Path::new(&file)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let path = out.map_or_else(|| dir.join(".svetra").join("view.html"), Into::into);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, svetra::understand::view::page(&doc, &src)));
    if let Err(e) = written {
        eprintln!("svetra: cannot write {}: {e}", path.display());
        return ExitCode::from(1);
    }
    ignore_scratch(&dir);
    println!("wrote {}", path.display());
    if open {
        let opener = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&path).status()
        } else if cfg!(windows) {
            std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(&path)
                .status()
        } else {
            std::process::Command::new("xdg-open").arg(&path).status()
        };
        if !opener.is_ok_and(|s| s.success()) {
            println!("Open it in a browser to see it.");
        }
    }
    ExitCode::SUCCESS
}

/// `svetra diff` and `svetra plan` (docs/change.md).
fn change(command: &str, args: &[String]) -> ExitCode {
    let (mut file, mut from, mut json, mut accept, mut at) = (None, None, false, Vec::new(), None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--from" | "--accept" | "--at" if command == "plan" || a == "--from" => {
                let Some(v) = it.next() else {
                    return usage(&format!("{a} needs a value"));
                };
                match a.as_str() {
                    "--from" => from = Some(v.clone()),
                    "--accept" => accept.extend(v.split(',').map(|k| k.trim().to_string())),
                    _ => at = Some(v.clone()),
                }
            }
            f if f.starts_with('-') => return usage(&format!("unknown option '{f}'")),
            f if file.is_none() => file = Some(f.to_string()),
            _ => return usage(&format!("{command} takes one file")),
        }
    }
    let file = file.unwrap_or_else(|| "svetra.yaml".into());
    let path = std::path::Path::new(&file);
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let new_src = match read(&file) {
        Ok(s) => s,
        Err(e) => return usage(&e),
    };
    let (old_src, from_name) = match &from {
        Some(f) => match read(f) {
            Ok(s) => (s, f.clone()),
            Err(e) => return usage(&e),
        },
        None => match svetra::change::committed(path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("svetra: {e}.");
                return ExitCode::from(2);
            }
        },
    };
    for (name, src) in [(&from_name, &old_src), (&file, &new_src)] {
        let diags = report::validate(src);
        if report::has_errors(&diags) {
            print!("{}", report::human(name, &diags));
            return ExitCode::from(1);
        }
    }
    let (old, _) = svetra::load::load(&old_src);
    let (new, _) = svetra::load::load(&new_src);
    let plan = svetra::change::plan(&old, &old_src, &new, &new_src);
    if !plan.diagnostics.is_empty() {
        let text = report::human(&file, &plan.diagnostics);
        if report::has_errors(&plan.diagnostics) {
            print!("{text}");
            return ExitCode::from(1);
        }
        eprint!("{text}");
    }
    let waiting: Vec<&str> = plan
        .needs_yes()
        .into_iter()
        .filter(|k| !accept.iter().any(|a| a == k))
        .collect();
    let blocked = command == "plan" && !waiting.is_empty();
    // The rename migration runs before `supabase db diff`, so the diff sees the new names and
    // writes nothing destructive for them. Only once nothing is waiting for a yes.
    let mut writes = Vec::new();
    if command == "plan" && !blocked && !plan.renames.is_empty() {
        let migrations = dir.join("supabase").join("migrations");
        let body = format!(
            "-- Written by svetra plan: renames declared with `was`, before supabase db diff.\n{}\n",
            plan.renames.join("\n")
        );
        let done = std::fs::read_dir(&migrations).ok().is_some_and(|d| {
            d.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .ends_with("_svetra_rename.sql")
                    && std::fs::read_to_string(e.path()).is_ok_and(|s| s == body)
            })
        });
        if !done {
            let stamp = at.unwrap_or_else(utc_stamp);
            let target = migrations.join(format!("{stamp}_svetra_rename.sql"));
            if let Err(e) =
                std::fs::create_dir_all(&migrations).and_then(|()| std::fs::write(&target, &body))
            {
                eprintln!("svetra: cannot write {}: {e}", target.display());
                return ExitCode::from(1);
            }
            writes.push(target.display().to_string());
        }
    }
    if json {
        let v = svetra::change::json(&plan, &from_name, &writes);
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        print!(
            "{}",
            svetra::change::human(&plan, &new.system.name, &from_name)
        );
        for w in &writes {
            println!("\nwrote {w} (run it before supabase db diff: it keeps the renamed data)");
        }
        if command == "plan" {
            if blocked {
                println!(
                    "\nNot ready: {} need{} a yes from the person who owns the data.",
                    waiting.join(", "),
                    if waiting.len() == 1 { "s" } else { "" }
                );
                println!(
                    "Once they agree, run again with --accept {}",
                    waiting.join(",")
                );
            } else if !plan.changes.is_empty() {
                println!(
                    "\nNext: svetra compile, then supabase db diff -f <name>, then supabase db push."
                );
            }
        }
    }
    if blocked {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// The current UTC time as YYYYMMDDHHMMSS, for a migration's file name only.
fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Add `.svetra/` to the project's .gitignore, when there is one that doesn't ignore it yet.
fn ignore_scratch(dir: &std::path::Path) {
    let gi = dir.join(".gitignore");
    let Ok(text) = std::fs::read_to_string(&gi) else {
        return;
    };
    if text
        .lines()
        .any(|l| matches!(l.trim(), ".svetra" | ".svetra/" | "/.svetra/"))
    {
        return;
    }
    let sep = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let _ = std::fs::write(&gi, format!("{text}{sep}.svetra/\n"));
}

fn usage(msg: &str) -> ExitCode {
    eprint!("svetra: {msg}\n\n{HELP}");
    ExitCode::from(2)
}
