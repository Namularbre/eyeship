//! `eyeship` binary: parses arguments and wires the adapters to the domain.
#![forbid(unsafe_code)]

mod cli;
mod commands;

use std::io;
use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    ExitCode::from(commands::run(
        &cli.command,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    ))
}
