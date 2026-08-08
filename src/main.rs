use std::process::ExitCode;

const HELP: &str = "bead-rs

Usage: bead [OPTIONS] <COMMAND>

This repository currently contains the clean-room specifications and project
scaffold. Runtime commands will be introduced only with their conformance
tests.

Options:
  -h, --help       Print help
  -V, --version    Print version
";

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-V" | "--version") => {
            println!("bead {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        None | Some("-h" | "--help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(command) => {
            eprintln!("bead: command not implemented in the specification scaffold: {command}");
            ExitCode::from(2)
        }
    }
}
