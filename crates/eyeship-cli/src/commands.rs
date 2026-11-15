//! Execution of each subcommand. Each one returns the process exit code.

use std::io::{self, Write};

use eyeship_domain::{RULES, Rule, rules};
use eyeship_report::{render_explain, render_rules};

use crate::cli::{Command, ComposeArgs};

/// No finding at or above the threshold.
pub const SUCCESS: u8 = 0;
/// Usage or read error.
pub const USAGE_ERROR: u8 = 2;

/// Runs a parsed command, writing results to `out` and errors to `err`.
pub fn run(command: &Command, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match command {
        Command::Compose(args) => run_compose(args, err),
        Command::Rules => run_rules(RULES, out, err),
        Command::Explain { id } => run_explain(rules::find(id), id, out, err),
    }
}

/// Audits a Compose project. Not wired to the loader yet.
pub fn run_compose(_args: &ComposeArgs, err: &mut dyn Write) -> u8 {
    report_error(err, "compose loading is not implemented yet")
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
        let output = capture(|out, err| run(&Command::Explain { id: "R999".into() }, out, err));
        assert_eq!(output.code, USAGE_ERROR);
        assert!(output.out.is_empty());
        assert_eq!(
            output.err,
            "error: unknown rule 'R999' (run `eyeship rules` to list the rules)\n"
        );
    }

    #[test]
    fn compose_is_not_implemented_yet() {
        let args = ComposeArgs {
            file: None,
            files: Vec::new(),
            env_file: None,
            format: OutputFormat::Text,
            fail_on: Threshold::Error,
            ignore: Vec::new(),
        };
        let output = capture(|out, err| run(&Command::Compose(args), out, err));
        assert_eq!(output.code, USAGE_ERROR);
        assert!(
            output
                .err
                .contains("compose loading is not implemented yet")
        );
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
