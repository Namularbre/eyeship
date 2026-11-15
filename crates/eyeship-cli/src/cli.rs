//! Command-line arguments (plan §6).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use eyeship_domain::FailOn;

/// Finds what is likely to bring your deployment down, before production.
#[derive(Debug, Parser)]
#[command(name = "eyeship", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Audit a Docker Compose project.
    Compose(ComposeArgs),
    /// List every rule with its default severity.
    Rules,
    /// Explain a rule: failure scenario, fix and examples.
    Explain {
        /// Rule ID, such as R001 (case-insensitive).
        id: String,
    },
}

#[derive(Debug, PartialEq, Eq, Args)]
pub struct ComposeArgs {
    /// Compose file to audit (default: the Compose lookup in the current directory).
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
    /// Additional Compose files, merged in order.
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    pub files: Vec<PathBuf>,
    /// File of variables used for interpolation.
    #[arg(long, value_name = "FILE")]
    pub env_file: Option<PathBuf>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
    /// Minimum severity that makes the audit fail (exit code 1).
    #[arg(long, value_enum, default_value_t = Threshold::Error)]
    pub fail_on: Threshold,
    /// Rule IDs to ignore, comma-separated.
    #[arg(long, value_name = "IDS", value_delimiter = ',')]
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

/// `--fail-on` values, mapped to the domain's [`FailOn`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Threshold {
    Error,
    Warning,
    Info,
    None,
}

impl From<Threshold> for FailOn {
    fn from(threshold: Threshold) -> Self {
        match threshold {
            Threshold::Error => Self::Error,
            Threshold::Warning => Self::Warning,
            Threshold::Info => Self::Info,
            Threshold::None => Self::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Command, clap::Error> {
        Cli::try_parse_from(std::iter::once("eyeship").chain(args.iter().copied()))
            .map(|cli| cli.command)
    }

    fn compose(args: &[&str]) -> ComposeArgs {
        match parse(&[&["compose"], args].concat()) {
            Ok(Command::Compose(args)) => args,
            other => panic!("expected compose arguments, got {other:?}"),
        }
    }

    #[test]
    fn the_cli_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn compose_defaults() {
        assert_eq!(
            compose(&[]),
            ComposeArgs {
                file: None,
                files: Vec::new(),
                env_file: None,
                format: OutputFormat::Text,
                fail_on: Threshold::Error,
                ignore: Vec::new(),
            }
        );
    }

    #[test]
    fn compose_parses_every_option() {
        let args = compose(&[
            "base.yaml",
            "-f",
            "a.yaml",
            "--file",
            "b.yaml",
            "--env-file",
            "prod.env",
            "--format",
            "json",
            "--fail-on",
            "warning",
            "--ignore",
            "R001,R002",
            "--ignore",
            "R003",
        ]);
        assert_eq!(args.file, Some(PathBuf::from("base.yaml")));
        assert_eq!(
            args.files,
            [PathBuf::from("a.yaml"), PathBuf::from("b.yaml")]
        );
        assert_eq!(args.env_file, Some(PathBuf::from("prod.env")));
        assert_eq!(args.format, OutputFormat::Json);
        assert_eq!(FailOn::from(args.fail_on), FailOn::Warning);
        assert_eq!(args.ignore, ["R001", "R002", "R003"]);
    }

    #[test]
    fn rejects_an_unknown_threshold() {
        assert!(parse(&["compose", "--fail-on", "loud"]).is_err());
    }

    #[test]
    fn parses_rules_and_explain() {
        assert_eq!(parse(&["rules"]).ok(), Some(Command::Rules));
        assert_eq!(
            parse(&["explain", "r001"]).ok(),
            Some(Command::Explain { id: "r001".into() })
        );
    }

    #[test]
    fn threshold_none_never_fails() {
        assert_eq!(FailOn::from(Threshold::None), FailOn::None);
        assert_eq!(FailOn::from(Threshold::Info), FailOn::Info);
        assert_eq!(FailOn::from(Threshold::Error), FailOn::Error);
    }
}
