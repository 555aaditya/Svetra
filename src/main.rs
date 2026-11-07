use std::process::ExitCode;
use svetra::report;

const HELP: &str = "svetra: design the system with the AI, compile it to the low level

Usage:
  svetra validate [FILE] [--json]   Check a svetra.yaml file (default: ./svetra.yaml)
  svetra explain [FILE]             Describe the app in plain English: data, rules, who can see what
  svetra graph [FILE]               Draw the app as a Mermaid diagram
  svetra context [FILE]             Print the whole app compactly, for agents
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
        Some("explain") => view("explain", &args[1..], |doc, diags| {
            svetra::understand::explain::explain(doc, diags)
        }),
        Some("graph") => view("graph", &args[1..], |doc, _| {
            svetra::understand::graph::graph(doc)
        }),
        Some("context") => view("context", &args[1..], |doc, _| {
            svetra::understand::context::context(doc)
        }),
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

/// A read-only view of a valid file: validate first, then print `render`'s output.
fn view(
    command: &str,
    args: &[String],
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
    if report::has_errors(&diags) {
        print!("{}", report::human(file, &diags));
        return ExitCode::from(1);
    }
    let (doc, _) = svetra::load::load(&src);
    print!("{}", render(&doc, &diags));
    ExitCode::SUCCESS
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

fn usage(msg: &str) -> ExitCode {
    eprint!("svetra: {msg}\n\n{HELP}");
    ExitCode::from(2)
}
