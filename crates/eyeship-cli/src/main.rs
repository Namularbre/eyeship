//! `eyeship` binary: parses arguments and wires the adapters to the domain.
#![forbid(unsafe_code)]

mod cli;
mod commands;
mod config;

use std::env;
use std::io::{self, IsTerminal};
use std::process::ExitCode;

use clap::Parser;
use eyeship_compose::{ProcessEnv, RealFileSystem};
use eyeship_report::should_color;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let no_color = env::var("NO_COLOR").ok();
    let context = commands::Context {
        fs: &RealFileSystem,
        env: &ProcessEnv,
        working_dir: env::current_dir().unwrap_or_default(),
        color: should_color(io::stdout().is_terminal(), no_color.as_deref()),
    };
    ExitCode::from(commands::run(
        &cli.command,
        &context,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    ))
}
