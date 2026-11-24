use std::process::ExitCode;
use svetra::report;

const HELP: &str = "svetra: design the system with the AI, compile it to the low level

Usage:
  svetra validate [FILE] [--json]   Check a svetra.yaml file (default: ./svetra.yaml)
  svetra explain [FILE]             Describe the app in plain English: data, rules, who can see what
  svetra graph [FILE]               Draw the app as a Mermaid diagram
  svetra context [FILE]             Print the whole app compactly, for agents
  svetra diff [FILE] [--from OLD | --from-git REF] [--json]
                                    List the changes since OLD (default: FILE in git HEAD),
                                    classified: safe, breaking or destructive, and whose
                                    access widens or narrows
  svetra plan [FILE] [--from OLD | --from-git REF] [--json | --markdown] [--accept KIND,...]
              [--no-write] [--at YYYYMMDDHHMMSS]
                                    The same, as a plan: exits 1 on destructive changes or
                                    wider access until they're accepted, then writes the
                                    migration for any declared renames
  svetra patch [FILE] [--ops FILE | -] [--expected-revision REV] [--dry-run] [--json]
                                    Change the design with checked edits (set, remove, rename on
                                    a path), read as JSON from --ops or stdin. All or nothing:
                                    the result is validated, then written. Exits 1 if refused,
                                    3 if FILE is not at REV (it changed since you read it)
  svetra check (--db URL | --local | --snapshot FILE) [--schema public,...] [--json]
  svetra check --migrations [DIR] [--scratch URL] [--strict] [--schema public,...] [--json]
                                    Find the leaks in an existing database: tables anyone can
                                    read, open policies, views and functions that skip
                                    row-level security. --print-query prints the read-only
                                    query, to run in the Supabase SQL editor without psql
  svetra infer (--db URL | --local | --snapshot FILE) [--schema public,...] [--out FILE] [--force]
                                    Draft a svetra.yaml from an existing database: entities,
                                    fields, constraints, indexes. Whatever it can't translate
                                    stays as an UNTRANSLATED comment; check's findings go in
                                    comments too. --print-query prints the read-only query
  svetra infer --check-draft FILE (--migrations [DIR] | --db URL | --local) [--schema public,...]
                                    Run the draft's access tests against a throwaway copy of
                                    the original database (Docker) and list where they
                                    disagree. Never touches the original
  svetra doctor [DIR] [--db URL | --local | --migrations [DIR] [--scratch URL] [--strict]]
                [--schema public,...] [--json]
                                    The pre-launch checklist: service keys and other secrets
                                    in code, committed .env files, secrets in
                                    supabase/config.toml, generated files out of date, and
                                    with a database source everything `check` finds. Reads
                                    only, prints no secret's value, never says ready while
                                    a part is unchecked (auth settings and buckets are not
                                    read yet)
  svetra view [FILE] [--out PATH] [--no-open] [--json]
                                    Write one page showing who can see and change what
                                    (default .svetra/view.html) and open it; --json prints
                                    the view model instead
  svetra compile [FILE] [--check]   Write the SQL schema, TypeScript types and access tests next
                                    to FILE, and sign-in settings into supabase/config.toml;
                                    --check writes nothing and fails if they are out of date
  svetra new <TEMPLATE> [DIR]       Start from a template: writes a svetra.yaml with comments
                                    in plain words. `svetra new --list` shows the templates
  svetra init [DIR]                 Set up a project for agents: the Svetra skill, the MCP
                                    server in .mcp.json, and a starter svetra.yaml
  svetra mcp                        Serve validate, compile, explain, context and graph to
                                    agents over MCP (stdio)
  svetra --version                  Print the version
  svetra --help                     Print this help

Exit codes: 0 ok, 1 errors found (or stale files with --check; a critical finding with
doctor), 2 usage error.
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
        Some("new") => new(&args[1..]),
        Some("explain") => view("explain", &args[1..], false, |doc, diags, _| {
            svetra::understand::explain::explain(doc, diags)
        }),
        Some("graph") => view("graph", &args[1..], true, |doc, _, _| {
            svetra::understand::graph::graph(doc)
        }),
        // The first line carries the file's revision, which `patch --expected-revision` takes.
        Some("context") => view("context", &args[1..], true, |doc, _, src| {
            let out = svetra::understand::context::context(doc);
            match out.split_once('\n') {
                Some((head, rest)) => format!(
                    "{head}, revision {}\n{rest}",
                    svetra::compile::revision(src)
                ),
                None => out,
            }
        }),
        Some("doctor") => doctor(&args[1..]),
        Some("view") => page(&args[1..]),
        Some(cmd @ ("check" | "infer")) => inspect(cmd, &args[1..]),
        Some(cmd @ ("diff" | "plan")) => change(cmd, &args[1..]),
        Some("patch") => patch(&args[1..]),
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
    render: impl Fn(&svetra::model::Document, &[svetra::diag::Diagnostic], &str) -> String,
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
    print!("{}", render(&doc, &diags, &src));
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
        Err(Failure::HoleBodies(bodies)) => {
            for (path, d) in &bodies {
                print!("{}", report::human(path, std::slice::from_ref(d)));
            }
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
        Err(_) => unreachable!("init neither validates nor reads hole bodies"),
    }
}

/// `svetra doctor [DIR] [--db URL | --local | --migrations [DIR]] ...` (docs/doctor.md).
fn doctor(args: &[String]) -> ExitCode {
    use svetra::inspect::{CheckError, Source};
    let (mut json, mut strict, mut local) = (false, false, false);
    let (mut db, mut scratch, mut migrations): (Option<String>, Option<String>, Option<String>) =
        (None, None, None);
    let mut migrations_given = false;
    let mut schemas = vec!["public".to_string()];
    let mut dirs: Vec<&String> = Vec::new();
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--strict" => strict = true,
            "--local" => local = true,
            "--migrations" => {
                migrations_given = true;
                if let Some(v) = it.next_if(|v| !v.starts_with("--")) {
                    migrations = Some(v.clone());
                }
            }
            "--db" | "--scratch" | "--schema" => {
                let Some(v) = it.next() else {
                    return usage(&format!("{a} needs a value"));
                };
                match a.as_str() {
                    "--db" => db = Some(v.clone()),
                    "--scratch" => scratch = Some(v.clone()),
                    _ => {
                        schemas = v
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    }
                }
            }
            other if other.starts_with('-') => {
                return usage(&format!("unknown option: '{other}'"));
            }
            _ => dirs.push(a),
        }
    }
    if dirs.len() > 1 {
        return usage("doctor takes one folder");
    }
    if [db.is_some(), local, migrations_given]
        .iter()
        .filter(|b| **b)
        .count()
        > 1
    {
        return usage("give at most one of --db URL, --local and --migrations");
    }
    if !migrations_given && (scratch.is_some() || strict) {
        return usage("--scratch and --strict go with --migrations");
    }
    let dir = dirs.first().map_or(".", |d| d.as_str());
    let path = std::path::Path::new(dir);
    let source = if migrations_given {
        let m =
            migrations.unwrap_or_else(|| path.join("supabase/migrations").display().to_string());
        Some(Source::Migrations { dir: m, scratch })
    } else if local {
        Some(Source::Db(
            "postgresql://postgres:postgres@127.0.0.1:54322/postgres".to_string(),
        ))
    } else {
        db.map(Source::Db)
    };
    match svetra::doctor::run(path, source.as_ref(), &schemas) {
        Ok(report) => {
            print!("{}", if json { report.json() } else { report.human() });
            ExitCode::from(report.exit_code(strict))
        }
        Err(CheckError::Usage(m)) => usage(&m),
        Err(CheckError::Fail(m)) => {
            eprintln!("svetra: {m}");
            ExitCode::from(2)
        }
    }
}

/// `svetra new <template> [DIR]` and `svetra new --list` (docs/new.md).
fn new(args: &[String]) -> ExitCode {
    use svetra::project::{NewFailure, TEMPLATES};
    let names = || TEMPLATES.iter().map(|t| t.0).collect::<Vec<_>>().join(", ");
    let (name, dir) = match args {
        [f] if f == "--list" => {
            let width = TEMPLATES.iter().map(|t| t.0.len()).max().unwrap_or(0);
            for (name, about, _) in TEMPLATES {
                println!("{name:<width$}  {about}");
            }
            return ExitCode::SUCCESS;
        }
        [n] if !n.starts_with('-') => (n.as_str(), "."),
        [n, d] if !n.starts_with('-') && !d.starts_with('-') => (n.as_str(), d.as_str()),
        [] => return usage(&format!("new needs a template: {}", names())),
        _ => return usage("new takes a template and an optional folder, or --list"),
    };
    let about = match svetra::project::new_project(name, std::path::Path::new(dir)) {
        Ok(about) => about,
        Err(NewFailure::Unknown(n)) => {
            return usage(&format!("no template called '{n}'. Templates: {}", names()));
        }
        Err(NewFailure::Exists) => {
            eprintln!(
                "svetra: svetra.yaml already exists here; `svetra new` never replaces your design. Pick another folder, or move the file first."
            );
            return ExitCode::from(2);
        }
        Err(NewFailure::Write(e)) => {
            eprintln!("svetra: {e}");
            return ExitCode::from(2);
        }
    };
    let file = std::path::Path::new(dir).join("svetra.yaml");
    let diags = report::validate(&std::fs::read_to_string(&file).unwrap_or_default());
    if report::has_errors(&diags) {
        print!("{}", report::human(&file.display().to_string(), &diags));
        return ExitCode::from(1);
    }
    println!("wrote {} ({name}: {about})", file.display());
    println!("Next:");
    if dir != "." {
        println!("  cd {dir}");
    }
    println!("  svetra explain      read the rules in plain English");
    println!("  svetra compile      write the SQL, types and access tests");
    println!("  svetra init         set up your coding agent, when you want it to take over");
    ExitCode::SUCCESS
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

/// `svetra check` (docs/check.md) and `svetra infer` (docs/infer.md): both read one catalog
/// snapshot, `check` to find the leaks in it and `infer` to draft a `svetra.yaml` from it.
fn inspect(command: &str, args: &[String]) -> ExitCode {
    let infer = command == "infer";
    if infer && args.iter().any(|a| a == "--check-draft") {
        return check_draft(args);
    }
    let (mut db, mut snapshot, mut json, mut print) = (None, None, false, false);
    let (mut migrations, mut scratch, mut strict): (Option<String>, Option<String>, bool) =
        (None, None, false);
    let (mut out, mut force) = (None, false);
    let mut schemas = vec!["public".to_string()];
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" if !infer => json = true,
            "--force" if infer => force = true,
            "--strict" if !infer => strict = true,
            "--migrations" if !infer => {
                let dir = match it.peek() {
                    Some(v) if !v.starts_with("--") => it.next().cloned().unwrap_or_default(),
                    _ => "supabase/migrations".to_string(),
                };
                migrations = Some(dir);
            }
            "--print-query" => print = true,
            "--local" => {
                db = Some("postgresql://postgres:postgres@127.0.0.1:54322/postgres".to_string())
            }
            "--db" | "--snapshot" | "--schema" | "--scratch" | "--out"
                if (a != "--out" || infer) && (a != "--scratch" || !infer) =>
            {
                let Some(v) = it.next() else {
                    return usage(&format!("{a} needs a value"));
                };
                match a.as_str() {
                    "--db" => db = Some(v.clone()),
                    "--scratch" => scratch = Some(v.clone()),
                    "--snapshot" => snapshot = Some(v.clone()),
                    "--out" => out = Some(v.clone()),
                    _ => {
                        schemas = v
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    }
                }
            }
            other => return usage(&format!("unknown argument '{other}' for {command}")),
        }
    }
    if migrations.is_none() && (scratch.is_some() || strict) {
        return usage("--scratch and --strict go with --migrations");
    }
    if migrations.is_some() && (db.is_some() || snapshot.is_some() || print) {
        return usage(
            "--migrations can't be combined with --db, --local, --snapshot or --print-query",
        );
    }
    if print {
        let query = if infer {
            svetra::inspect::infer_query(&schemas)
        } else {
            svetra::inspect::query(&schemas)
        };
        println!("{query};");
        return ExitCode::SUCCESS;
    }
    if infer {
        return infer_draft(db, snapshot, &schemas, out, force);
    }
    let source = match (migrations, db, snapshot) {
        (Some(dir), _, _) => svetra::inspect::Source::Migrations { dir, scratch },
        (None, Some(url), None) => svetra::inspect::Source::Db(url),
        (None, None, Some(f)) => svetra::inspect::Source::Snapshot(f),
        _ => return usage("check needs one of --db URL, --local or --snapshot FILE"),
    };
    let report = match svetra::inspect::run(&source, &schemas) {
        Ok(r) => r,
        Err(svetra::inspect::CheckError::Usage(m)) => return usage(&m),
        Err(svetra::inspect::CheckError::Fail(m)) => {
            eprintln!("svetra: {m}");
            return ExitCode::from(2);
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report.json()).unwrap_or_default()
        );
    } else {
        print!("{}", report.human(&schemas));
    }
    ExitCode::from(report.exit_code(strict))
}

/// `svetra infer --check-draft FILE` (docs/infer.md, Differential testing).
fn check_draft(args: &[String]) -> ExitCode {
    let (mut file, mut migrations, mut db) = (None, None, None);
    let mut schemas = vec!["public".to_string()];
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--local" => {
                db = Some("postgresql://postgres:postgres@127.0.0.1:54322/postgres".to_string())
            }
            "--migrations" => {
                migrations = Some(match it.peek() {
                    Some(v) if !v.starts_with("--") => it.next().cloned().unwrap_or_default(),
                    _ => "supabase/migrations".to_string(),
                })
            }
            "--check-draft" | "--db" | "--schema" => {
                let Some(v) = it.next() else {
                    return usage(&format!("{a} needs a value"));
                };
                match a.as_str() {
                    "--check-draft" => file = Some(v.clone()),
                    "--db" => db = Some(v.clone()),
                    _ => {
                        schemas = v
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    }
                }
            }
            other => {
                return usage(&format!(
                    "unknown argument '{other}' for infer --check-draft"
                ));
            }
        }
    }
    let Some(file) = file else {
        return usage("--check-draft needs a draft file");
    };
    if migrations.is_some() == db.is_some() {
        return usage("--check-draft needs exactly one of --migrations [DIR], --db URL or --local");
    }
    let yaml = match read(&file) {
        Ok(t) => t,
        Err(e) => return usage(&e),
    };
    let (analysis, entities) = match svetra::verify::prepare(&yaml) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("svetra: {e}");
            return ExitCode::from(2);
        }
    };
    let (copy, source) = match (&migrations, &db) {
        (Some(dir), _) => (
            svetra::migrations::Copy::from_migrations(std::path::Path::new(dir)),
            format!("built from {dir}"),
        ),
        (_, Some(url)) => (
            svetra::verify::dump(url, &schemas)
                .and_then(|sql| svetra::migrations::Copy::from_dump(&sql)),
            format!("schema of {}", svetra::inspect::redact(url)),
        ),
        _ => return ExitCode::from(2),
    };
    let copy = match copy {
        Ok(c) => c,
        Err(svetra::migrations::Fail(m)) => {
            eprintln!("svetra: {m}");
            return ExitCode::from(2);
        }
    };
    let ran = copy
        .run_tests(&analysis.text)
        .map_err(|svetra::migrations::Fail(m)| m)
        .and_then(|out| svetra::verify::judge_run(&analysis, &entities, &yaml, &out));
    match ran {
        Ok(report) => {
            print!("{}", report.human(&file, &source, &copy.notes));
            ExitCode::from(u8::from(report.disagreements() > 0))
        }
        Err(m) => {
            eprintln!("svetra: {m}");
            ExitCode::from(2)
        }
    }
}

/// `svetra infer` (docs/infer.md): read the snapshot with the infer query, then write the draft.
fn infer_draft(
    db: Option<String>,
    snapshot: Option<String>,
    schemas: &[String],
    out: Option<String>,
    force: bool,
) -> ExitCode {
    let text = match (&db, &snapshot) {
        (Some(url), None) => {
            let query = svetra::inspect::infer_query(schemas);
            let run = std::process::Command::new("psql")
                .args([
                    url.as_str(),
                    "-X",
                    "-A",
                    "-t",
                    "-q",
                    "-v",
                    "ON_ERROR_STOP=1",
                    "-c",
                    &query,
                ])
                .output();
            match run {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
                Ok(o) => {
                    eprintln!(
                        "svetra: the database refused the query: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    );
                    return ExitCode::from(2);
                }
                Err(_) => {
                    eprintln!(
                        "svetra: psql isn't installed. Run `svetra infer --print-query`, run the query in the Supabase SQL editor, save the JSON it returns to a file, then `svetra infer --snapshot <file>`."
                    );
                    return ExitCode::from(2);
                }
            }
        }
        (None, Some(f)) => match read(f) {
            Ok(t) => t,
            Err(e) => return usage(&e),
        },
        _ => return usage("infer needs one of --db URL, --local or --snapshot FILE"),
    };
    let snap: serde_json::Value = match serde_json::from_str(text.trim()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "svetra: that isn't the JSON the infer query returns ({e}). Run `svetra infer --print-query` for the query."
            );
            return ExitCode::from(2);
        }
    };
    draft(&snap, out.as_deref().unwrap_or("svetra.yaml"), force)
}

/// `svetra infer`: write the draft, or print it with `--out -` (docs/infer.md, Output).
fn draft(snap: &serde_json::Value, out: &str, force: bool) -> ExitCode {
    let d = match svetra::infer::infer(snap) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("svetra: {e}");
            return ExitCode::from(2);
        }
    };
    if out == "-" {
        print!("{}", d.yaml);
        return ExitCode::SUCCESS;
    }
    if std::path::Path::new(out).exists() && !force {
        eprintln!(
            "svetra: {out} already exists. Pass --force to overwrite it, or --out to write elsewhere."
        );
        return ExitCode::from(2);
    }
    if let Err(e) = std::fs::write(out, &d.yaml) {
        eprintln!("svetra: can't write {out}: {e}");
        return ExitCode::from(2);
    }
    println!(
        "wrote {out}: {} entities, {} fields, {} things left as UNTRANSLATED, {} critical findings in comments",
        d.entities, d.fields, d.untranslated, d.critical
    );
    println!("Next: review the draft, then `svetra validate`, then `svetra compile`.");
    ExitCode::SUCCESS
}

/// `svetra patch` (docs/patch.md).
fn patch(args: &[String]) -> ExitCode {
    use std::io::{IsTerminal, Read};
    let (mut file, mut ops, mut expected, mut dry_run, mut json) = (None, None, None, false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--dry-run" => dry_run = true,
            "--ops" | "--expected-revision" => {
                let Some(v) = it.next() else {
                    return usage(&format!("{a} needs a value"));
                };
                if a == "--ops" {
                    ops = Some(v.clone());
                } else {
                    expected = Some(v.clone());
                }
            }
            "-" if ops.is_none() => ops = Some("-".into()),
            f if f.starts_with('-') => return usage(&format!("unknown option '{f}'")),
            f if file.is_none() => file = Some(f.to_string()),
            _ => return usage("patch takes one file"),
        }
    }
    let file = file.unwrap_or_else(|| "svetra.yaml".into());
    let text = match ops.as_deref() {
        Some(path) if path != "-" => match read(path) {
            Ok(s) => s,
            Err(e) => return usage(&e),
        },
        None if std::io::stdin().is_terminal() => {
            return usage("patch needs the edits: pass --ops FILE, or pipe them in");
        }
        _ => {
            let mut s = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                return usage(&format!("cannot read the edits from stdin: {e}"));
            }
            s
        }
    };
    let edits = match svetra::patch::parse_ops(&text) {
        Ok(v) => v,
        Err(e) => return usage(&e),
    };
    let report = match svetra::patch::run(
        std::path::Path::new(&file),
        &edits,
        expected.as_deref(),
        dry_run,
    ) {
        Ok(r) => r,
        Err(e) => return usage(&e),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&svetra::patch::json(&report)).unwrap_or_default()
        );
    } else {
        print!("{}", svetra::patch::human(&report, edits.len()));
    }
    if report.conflict.is_some() {
        ExitCode::from(3)
    } else if report.applied {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// `svetra diff` and `svetra plan` (docs/change.md).
fn change(command: &str, args: &[String]) -> ExitCode {
    let (mut file, mut from, mut json, mut accept, mut at) = (None, None, false, Vec::new(), None);
    let (mut from_git, mut markdown, mut no_write) = (None, false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--markdown" | "--no-write" if command == "plan" => {
                if a == "--markdown" {
                    markdown = true;
                } else {
                    no_write = true;
                }
            }
            "--from" | "--from-git" | "--accept" | "--at"
                if command == "plan" || a == "--from" || a == "--from-git" =>
            {
                let Some(v) = it.next() else {
                    return usage(&format!("{a} needs a value"));
                };
                match a.as_str() {
                    "--from" => from = Some(v.clone()),
                    "--from-git" => from_git = Some(v.clone()),
                    "--accept" => accept.extend(v.split(',').map(|k| k.trim().to_string())),
                    _ => at = Some(v.clone()),
                }
            }
            f if f.starts_with('-') => return usage(&format!("unknown option '{f}'")),
            f if file.is_none() => file = Some(f.to_string()),
            _ => return usage(&format!("{command} takes one file")),
        }
    }
    if from.is_some() && from_git.is_some() {
        return usage("--from and --from-git can't be used together");
    }
    if json && markdown {
        return usage("--json and --markdown can't be used together");
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
    let (old_src, from_name) = match (&from, &from_git) {
        (Some(f), _) => match read(f) {
            Ok(s) => (s, f.clone()),
            Err(e) => return usage(&e),
        },
        (None, Some(rev)) => match svetra::change::committed_at(path, rev) {
            Ok(v) => v,
            Err(e) => return usage(&e),
        },
        (None, None) => match svetra::change::committed(path) {
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
    let mut would_write = Vec::new();
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
        if !done && no_write {
            would_write.push(
                migrations
                    .join("<time>_svetra_rename.sql")
                    .display()
                    .to_string(),
            );
        } else if !done {
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
    if markdown {
        let waiting_now: Vec<&str> = waiting.clone();
        let accepted: Vec<&str> = plan
            .needs_yes()
            .into_iter()
            .filter(|k| accept.iter().any(|a| a == k))
            .collect();
        print!(
            "{}",
            svetra::change::markdown(
                &plan,
                &new.system.name,
                &from_name,
                &waiting_now,
                &accepted,
                !plan.renames.is_empty()
            )
        );
    } else if json {
        let mut v = svetra::change::json(&plan, &from_name, &writes);
        if !would_write.is_empty() {
            v["would_write"] = serde_json::json!(would_write);
        }
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        print!(
            "{}",
            svetra::change::human(&plan, &new.system.name, &from_name)
        );
        for w in &writes {
            println!("\nwrote {w} (run it before supabase db diff: it keeps the renamed data)");
        }
        for w in &would_write {
            println!("\nwould write {w} (not written: --no-write)");
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
