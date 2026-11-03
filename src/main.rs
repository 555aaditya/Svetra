use std::process::ExitCode;
use svetra::report;

const HELP: &str = "svetra: design the system with the AI, compile it to the low level

Usage:
  svetra validate [FILE] [--json]   Check a svetra.yaml file (default: ./svetra.yaml)
  svetra --version                  Print the version
  svetra --help                     Print this help

Exit codes: 0 valid, 1 errors found, 2 usage error.
";

// ponytail: hand-rolled argument parsing for one command; switch to clap at the second.
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
        Some(other) => usage(&format!("unknown argument '{other}'")),
    }
}

fn validate(args: &[String]) -> ExitCode {
    let json = args.iter().any(|a| a == "--json");
    let files: Vec<&String> = args.iter().filter(|a| *a != "--json").collect();
    if let Some(flag) = files.iter().find(|a| a.starts_with('-')) {
        return usage(&format!("unknown option '{flag}'"));
    }
    if files.len() > 1 {
        return usage("validate takes one file");
    }
    let file = files.first().map_or("svetra.yaml", |f| f.as_str());
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => return usage(&format!("cannot read {file}: {e}")),
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

fn usage(msg: &str) -> ExitCode {
    eprint!("svetra: {msg}\n\n{HELP}");
    ExitCode::from(2)
}
