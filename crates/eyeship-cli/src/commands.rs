//! Execution of each subcommand. Each one returns the process exit code.

use std::io::{self, Write};
use std::path::PathBuf;

use eyeship_compose::{Env, FileSystem, LoadRequest, load};
use eyeship_domain::{AuditConfig, FailOn, RULES, Rule, audit, rules};
use eyeship_report::{Format, render, render_explain, render_rules};

use crate::cli::{Command, ComposeArgs, OutputFormat};
use crate::config::load_config;

/// No finding at or above the threshold.
pub const SUCCESS: u8 = 0;
/// Usage or read error.
pub const USAGE_ERROR: u8 = 2;

/// What the commands need from the outside world.
pub struct Context<'a> {
    pub fs: &'a dyn FileSystem,
    pub env: &'a dyn Env,
    pub working_dir: PathBuf,
    /// Whether text output may use colors.
    pub color: bool,
}

/// Runs a parsed command, writing results to `out` and errors to `err`.
pub fn run(command: &Command, context: &Context, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match command {
        Command::Compose(args) => run_compose(args, context, out, err),
        Command::Rules => run_rules(RULES, out, err),
        Command::Explain { id } => run_explain(rules::find(id), id, out, err),
    }
}

/// Audits a Compose project: load, audit, render, then the exit code.
pub fn run_compose(
    args: &ComposeArgs,
    context: &Context,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let files: Vec<&PathBuf> = args.file.iter().chain(&args.files).collect();
    if files.len() > 1 {
        return report_error(err, "auditing several merged files is not supported yet");
    }
    let config = match load_config(context.fs, &context.working_dir) {
        Ok(config) => config,
        Err(error) => return report_error(err, &error.to_string()),
    };
    let request = LoadRequest {
        file: files.first().map(|file| (*file).clone()),
        env_file: args.env_file.clone(),
        working_dir: context.working_dir.clone(),
    };
    match load(&request, context.fs, context.env) {
        Err(error) => report_error(err, &error.to_string()),
        Ok(project) => {
            let fail_on = args
                .fail_on
                .map(FailOn::from)
                .or(config.fail_on)
                .unwrap_or_default();
            let audit_config = AuditConfig {
                ignored_rules: config
                    .ignored_rules
                    .into_iter()
                    .chain(args.ignore.iter().map(|id| id.trim().to_uppercase()))
                    .collect(),
                ignored_for_service: config.ignored_for_service,
            };
            let report = audit(&project, RULES, &audit_config);
            let format = match args.format {
                OutputFormat::Text => Format::Text {
                    color: context.color,
                },
                OutputFormat::Json => Format::Json,
            };
            match render(&report, format, env!("CARGO_PKG_VERSION"), out) {
                Ok(()) => report.exit_code(fail_on),
                Err(error) => report_error(err, &format!("cannot write the output: {error}")),
            }
        }
    }
}

fn run_rules(rules: &[Rule], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    exit_code(render_rules(rules, out), err)
}

fn run_explain(rule: Option<&Rule>, id: &str, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match rule {
        Some(rule) => exit_code(render_explain(rule, out), err),
        None => report_error(
            err,
            &format!("unknown rule '{id}' (run `eyeship rules` to list the rules)"),
        ),
    }
}

fn exit_code(written: io::Result<()>, err: &mut dyn Write) -> u8 {
    match written {
        Ok(()) => SUCCESS,
        Err(error) => report_error(err, &format!("cannot write the output: {error}")),
    }
}

fn report_error(err: &mut dyn Write, message: &str) -> u8 {
    // Nothing more can be done if stderr itself is unwritable.
    let _ = writeln!(err, "error: {message}");
    USAGE_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyeship_domain::{Finding, Project, RuleDoc, RuleId, Severity};

    use std::path::PathBuf;

    use eyeship_compose::{MemoryEnv, MemoryFileSystem};

    use crate::cli::{OutputFormat, Threshold};

    fn no_findings(_: &Project) -> Vec<Finding> {
        Vec::new()
    }

    const RULE: Rule = Rule {
        id: RuleId("R042"),
        title: "a test rule",
        severity: Severity::Warning,
        doc: RuleDoc {
            scenario: "It crashes.",
            fix: "Do not crash.",
            bad_example: "bad: true",
            good_example: "bad: false",
        },
        check: no_findings,
    };

    struct Output {
        code: u8,
        out: String,
        err: String,
    }

    fn capture(run: impl FnOnce(&mut dyn Write, &mut dyn Write) -> u8) -> Output {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(&mut out, &mut err);
        Output {
            code,
            out: String::from_utf8_lossy(&out).into_owned(),
            err: String::from_utf8_lossy(&err).into_owned(),
        }
    }

    #[test]
    fn rules_lists_the_given_rules() {
        let output = capture(|out, err| run_rules(&[RULE], out, err));
        assert_eq!(output.code, SUCCESS);
        assert!(output.out.contains("R042"));
        assert!(output.err.is_empty());
    }

    #[test]
    fn explain_shows_a_known_rule() {
        let output = capture(|out, err| run_explain(Some(&RULE), "r042", out, err));
        assert_eq!(output.code, SUCCESS);
        assert!(output.out.contains("It crashes."));
    }

    #[test]
    fn explain_rejects_an_unknown_rule_on_stderr() {
        let output = capture(|out, err| run_explain(None, "R999", out, err));
        assert_eq!(output.code, USAGE_ERROR);
        assert!(output.out.is_empty());
        assert_eq!(
            output.err,
            "error: unknown rule 'R999' (run `eyeship rules` to list the rules)\n"
        );
    }

    const CLEAN: &str = "services:\n  web:\n    image: nginx:1.27\n    restart: unless-stopped\n    mem_limit: 256m\n    logging:\n      driver: json-file\n      options:\n        max-size: 10m\n";
    const CONFLICT: &str = "services:\n  a:\n    image: nginx:1.27\n    ports: [\"80:80\"]\n  b:\n    image: nginx:1.27\n    ports: [\"80:8080\"]\n";

    fn compose_args(file: Option<&str>) -> ComposeArgs {
        ComposeArgs {
            file: file.map(PathBuf::from),
            files: Vec::new(),
            env_file: None,
            format: OutputFormat::Text,
            fail_on: None,
            ignore: Vec::new(),
        }
    }

    fn run_on(content: &str, args: &ComposeArgs) -> Output {
        run_with_config(content, None, args)
    }

    fn run_with_config(content: &str, config: Option<&str>, args: &ComposeArgs) -> Output {
        let fs = MemoryFileSystem::new(
            std::iter::once(("/work/compose.yaml", content))
                .chain(config.map(|config| ("/work/.eyeship.toml", config))),
        );
        let env = MemoryEnv::new(Vec::<(String, String)>::new());
        let context = Context {
            fs: &fs,
            env: &env,
            working_dir: PathBuf::from("/work"),
            color: false,
        };
        capture(|out, err| run(&Command::Compose(args.clone()), &context, out, err))
    }

    #[test]
    fn a_clean_project_passes() {
        let output = run_on(CLEAN, &compose_args(None));
        assert_eq!(output.code, SUCCESS, "{}", output.out);
        assert!(output.out.contains("No issues found."));
    }

    #[test]
    fn an_error_finding_fails_by_default() {
        let output = run_on(CONFLICT, &compose_args(None));
        assert_eq!(output.code, 1);
        assert!(output.out.contains("R009"));
    }

    #[test]
    fn fail_on_none_never_fails() {
        let args = ComposeArgs {
            fail_on: Some(Threshold::None),
            ..compose_args(None)
        };
        assert_eq!(run_on(CONFLICT, &args).code, SUCCESS);
    }

    #[test]
    fn ignored_rules_are_counted_not_failed() {
        let args = ComposeArgs {
            ignore: vec!["r009".into()],
            ..compose_args(None)
        };
        let output = run_on(CONFLICT, &args);
        assert_eq!(output.code, SUCCESS);
        assert!(output.out.contains("ignored by configuration"));
    }

    #[test]
    fn a_missing_file_is_a_usage_error() {
        let output = run_on(CLEAN, &compose_args(Some("missing.yaml")));
        assert_eq!(output.code, USAGE_ERROR);
        assert!(output.out.is_empty());
        assert!(output.err.starts_with("error: "));
    }

    #[test]
    fn several_files_are_not_supported_yet() {
        let args = ComposeArgs {
            files: vec![PathBuf::from("a.yaml"), PathBuf::from("b.yaml")],
            ..compose_args(None)
        };
        assert_eq!(run_on(CLEAN, &args).code, USAGE_ERROR);
    }

    #[test]
    fn the_config_file_ignores_rules_globally() {
        let output = run_with_config(CONFLICT, Some("ignore = [\"R009\"]\n"), &compose_args(None));
        assert_eq!(output.code, SUCCESS, "{}", output.out);
        assert!(output.out.contains("ignored by configuration"));
    }

    #[test]
    fn the_config_file_ignores_rules_per_service() {
        let config = "[services.a]\nignore = [\"R009\"]\n[services.b]\nignore = [\"R009\"]\n";
        let output = run_with_config(CONFLICT, Some(config), &compose_args(None));
        assert_eq!(output.code, SUCCESS, "{}", output.out);
        let other = "[services.other]\nignore = [\"R009\"]\n";
        assert_eq!(
            run_with_config(CONFLICT, Some(other), &compose_args(None)).code,
            1
        );
    }

    #[test]
    fn the_config_threshold_applies_unless_overridden() {
        let config = Some("fail_on = \"none\"\n");
        assert_eq!(
            run_with_config(CONFLICT, config, &compose_args(None)).code,
            SUCCESS
        );
        let args = ComposeArgs {
            fail_on: Some(Threshold::Error),
            ..compose_args(None)
        };
        assert_eq!(run_with_config(CONFLICT, config, &args).code, 1);
    }

    #[test]
    fn cli_ignores_add_to_the_config_file() {
        let args = ComposeArgs {
            ignore: vec!["R009".into()],
            ..compose_args(None)
        };
        let output = run_with_config(CONFLICT, Some("ignore = [\"R001\"]\n"), &args);
        assert_eq!(output.code, SUCCESS, "{}", output.out);
    }

    #[test]
    fn an_invalid_config_file_is_a_usage_error() {
        let output = run_with_config(CLEAN, Some("ignore = [\"R999\"]\n"), &compose_args(None));
        assert_eq!(output.code, USAGE_ERROR);
        assert!(output.out.is_empty());
        assert!(output.err.contains("/work/.eyeship.toml"), "{}", output.err);
        assert!(output.err.contains("R999"), "{}", output.err);
    }

    struct Broken;

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_write_failure_is_reported() {
        let mut err = Vec::new();
        assert_eq!(run_rules(&[RULE], &mut Broken, &mut err), USAGE_ERROR);
        assert!(String::from_utf8_lossy(&err).contains("cannot write the output"));
    }
}
