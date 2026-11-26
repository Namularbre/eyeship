//! Command-line arguments (plan §6).

use std::net::IpAddr;
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
    /// Open the local web interface: deployments, reports and their history.
    Ui(UiArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct UiArgs {
    /// Address to listen on. Anything but a loopback address exposes the
    /// reports to the network.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: IpAddr,
    /// Port to listen on (0 picks a free one).
    #[arg(long, default_value_t = 7878)]
    pub port: u16,
    /// Directory of saved reports [default: `reports_dir` from `.eyeship.toml`,
    /// or .eyeship/reports].
    #[arg(long, value_name = "DIR")]
    pub reports_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Args)]
pub struct ComposeArgs {
    /// Compose file, or directory holding one, to audit (default: the current directory).
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
    /// Audit a deployment declared in `.eyeship.toml` instead of explicit files.
    #[arg(long, value_name = "NAME", conflicts_with_all = ["file", "files", "env_file"])]
    pub deployment: Option<String>,
    /// Additional Compose files, merged in order.
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    pub files: Vec<PathBuf>,
    /// File of variables used for interpolation.
    #[arg(long, value_name = "FILE")]
    pub env_file: Option<PathBuf>,
    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
    /// Minimum severity that makes the audit fail (exit code 1) [default: error, or
    /// `fail_on` from `.eyeship.toml`].
    #[arg(long, value_enum)]
    pub fail_on: Option<Threshold>,
    /// Rule IDs to ignore, comma-separated (added to those of `.eyeship.toml`).
    #[arg(long, value_name = "IDS", value_delimiter = ',')]
    pub ignore: Vec<String>,
    /// Also save the report (SARIF) in the deployment's history.
    #[arg(long)]
    pub save: bool,
    /// Label of the saved report, such as a commit SHA.
    #[arg(long, value_name = "TEXT", requires = "save")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    /// SARIF 2.1.0, for GitHub Code Scanning and other SARIF readers.
    Sarif,
    /// A self-contained HTML page, to read without eyeship.
    Html,
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
                fail_on: None,
                ignore: Vec::new(),
                deployment: None,
                save: false,
                label: None,
            }
        );
    }

    #[test]
    fn parses_ui() {
        assert_eq!(
            parse(&["ui"]).ok(),
            Some(Command::Ui(UiArgs {
                host: IpAddr::from([127, 0, 0, 1]),
                port: 7878,
                reports_dir: None,
            }))
        );
        assert_eq!(
            parse(&["ui", "--host", "::1", "--port", "0", "--reports-dir", "ci"]).ok(),
            Some(Command::Ui(UiArgs {
                host: "::1".parse().expect("an address"),
                port: 0,
                reports_dir: Some(PathBuf::from("ci")),
            }))
        );
        assert!(parse(&["ui", "--host", "localhost"]).is_err());
    }

    #[test]
    fn a_label_requires_save() {
        assert!(parse(&["compose", "--label", "abc"]).is_err());
        let args = compose(&["--save", "--label", "abc"]);
        assert!(args.save);
        assert_eq!(args.label, Some("abc".into()));
    }

    #[test]
    fn parses_a_deployment() {
        assert_eq!(
            compose(&["--deployment", "prod"]).deployment,
            Some("prod".into())
        );
    }

    #[test]
    fn a_deployment_excludes_explicit_files() {
        for extra in [
            &["compose.yaml"][..],
            &["-f", "a.yaml"],
            &["--env-file", ".env"],
        ] {
            let args = [&["compose", "--deployment", "prod"][..], extra].concat();
            assert!(parse(&args).is_err(), "{extra:?} must conflict");
        }
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
        assert_eq!(args.fail_on.map(FailOn::from), Some(FailOn::Warning));
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
