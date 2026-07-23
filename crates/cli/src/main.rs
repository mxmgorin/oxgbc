//! `oxgbc-cli` — a thin headless runner.
//!
//! It drives `core::harness` (the same boot + pass/fail detection the
//! integration tests use) so an arbitrary ROM can be run outside `cargo test`,
//! screenshotted, and batch-scored. Commands:
//!
//! ```text
//! oxgbc-cli run   <ROM> [--model ..] [--timeout ..] [--protocol ..] [--no-detect] [--screenshot P] [--serial]
//! oxgbc-cli check <DIR> [--model ..] [--timeout ..] [--protocol ..] [-r] [--exclude G] [--json] [--screenshot-dir D]
//! oxgbc-cli score [SUITE...] [--out DIR] [--model ..] [--timeout ..]
//! oxgbc-cli bench <ROM> [--frames N] [--repeat K] [--json] [--model ..]
//! oxgbc-cli bench matrix [--repeat K] [--model ..]
//! ```

mod args;
mod commands;
mod inspect;
mod report;
mod rom;

use crate::commands::{cmd_bench, cmd_check, cmd_run, cmd_score};
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();

    match dispatch(&argv) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}\n");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn dispatch(argv: &[String]) -> Result<ExitCode, String> {
    match argv.get(1).map(String::as_str) {
        Some("run") => cmd_run(&argv[2..]),
        Some("check") => cmd_check(&argv[2..]),
        Some("score") => cmd_score(&argv[2..]),
        Some("bench") => cmd_bench(&argv[2..]),
        Some("-h") | Some("--help") | Some("help") => {
            print_usage();
            Ok(ExitCode::SUCCESS)
        }
        Some(other) => Err(format!("unknown command '{other}'")),
        None => {
            print_usage();
            Ok(ExitCode::from(2))
        }
    }
}

/// Map an "all good?" flag to a process exit code (0 = success, 1 = failure).
pub fn exit_code(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The global usage: overview plus every command's option block. Each command
/// keeps its own block next to its parser and prints only that on `<cmd> -h`.
pub fn print_usage() {
    eprintln!("oxgbc-cli — headless Game Boy test-ROM runner\n");
    eprintln!("USAGE:");
    eprintln!("  oxgbc-cli run   <ROM> [options]");
    eprintln!("  oxgbc-cli check <DIR> [options]");
    eprintln!("  oxgbc-cli score [SUITE...] [options]");
    eprintln!("  oxgbc-cli bench <ROM> [options]\n");
    args::print_common_usage();
    commands::run::print_options();
    commands::check::print_options();
    commands::score::print_options();
    commands::bench::print_options();
}
