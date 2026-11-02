use std::process::ExitCode;

const HELP: &str = "svetra: design the system with the AI, compile it to the low level

Usage: svetra [--version | --help]

Options:
  -V, --version   Print the version
  -h, --help      Print this help
";

// ponytail: hand-rolled flag parsing, switch to clap when real subcommands land.
fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("svetra {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        None | Some("--help" | "-h") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprint!("svetra: unknown argument '{other}'\n\n{HELP}");
            ExitCode::from(2)
        }
    }
}
