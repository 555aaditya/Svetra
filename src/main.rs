use std::process::ExitCode;
use svetra::report;

const HELP: &str = "svetra: design the system with the AI, compile it to the low level

Usage:
  svetra validate [FILE] [--json]   Check a svetra.yaml file (default: ./svetra.yaml)
  svetra explain [FILE]             Describe the app in plain English: data, rules, who can see what
  svetra compile [FILE] [--check]   Write the SQL schema and TypeScript types next to FILE;
                                    --check writes nothing and fails if they are out of date
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
        Some("explain") => explain(&args[1..]),
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

fn explain(args: &[String]) -> ExitCode {
    let (file, _) = match file_and_flag("explain", args, "--no-such-flag") {
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
    print!("{}", svetra::understand::explain::explain(&doc, &diags));
    ExitCode::SUCCESS
}

fn compile(args: &[String]) -> ExitCode {
    let (file, check) = match file_and_flag("compile", args, "--check") {
        Ok(v) => v,
        Err(e) => return usage(&e),
    };
    let src = match read(file) {
        Ok(s) => s,
        Err(e) => return usage(&e),
    };
    let path = std::path::Path::new(file);
    let name = path
        .file_name()
        .map_or(file.into(), |n| n.to_string_lossy());
    let (outputs, notes) = match svetra::compile::compile(&src, &name) {
        Ok(v) => v,
        Err(diags) => {
            print!("{}", report::human(file, &diags));
            println!("Nothing was written.");
            return ExitCode::from(1);
        }
    };
    if !notes.is_empty() {
        print!("{}", report::human(file, &notes));
    }
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    if check {
        let stale: Vec<&str> = outputs
            .iter()
            .filter(|o| {
                std::fs::read_to_string(dir.join(o.path)).ok().as_deref()
                    != Some(o.contents.as_str())
            })
            .map(|o| o.path)
            .collect();
        if stale.is_empty() {
            println!("Generated files are up to date.");
            return ExitCode::SUCCESS;
        }
        for p in &stale {
            println!("out of date: {p}");
        }
        println!(
            "Run `svetra compile` to regenerate. Generated files must never be edited by hand."
        );
        return ExitCode::from(1);
    }
    for o in &outputs {
        if let Err(e) = write_atomic(&dir.join(o.path), &o.contents) {
            eprintln!("svetra: cannot write {}: {e}", o.path);
            return ExitCode::from(1);
        }
        println!("wrote {}", o.path);
    }
    ExitCode::SUCCESS
}

/// Write to a temporary file in the same folder, then rename, so a crash never leaves half a file.
fn write_atomic(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("svetra-tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

fn usage(msg: &str) -> ExitCode {
    eprint!("svetra: {msg}\n\n{HELP}");
    ExitCode::from(2)
}
