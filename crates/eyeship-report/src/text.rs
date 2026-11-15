//! Human-readable text output, grouped by service.

use std::io::{self, Write};

use crate::file_name;

use eyeship_domain::{Finding, Report, Severity, Summary};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[1;31m";
const YELLOW: &str = "\x1b[1;33m";
const BLUE: &str = "\x1b[1;34m";

/// Header of the group of findings that concern no particular service.
const PROJECT_GROUP: &str = "(project)";

/// Indentation of the lines under a finding's heading.
const DETAIL_INDENT: &str = "          ";

/// Writes `report` as text, with ANSI colors when `color` is true.
pub fn render_text(report: &Report, color: bool, out: &mut dyn Write) -> io::Result<()> {
    out.write_all(to_text(report, color).as_bytes())
}

/// Whether to color the output: only on a terminal, and never when
/// `NO_COLOR` is set to a non-empty value (<https://no-color.org>).
pub fn should_color(is_tty: bool, no_color: Option<&str>) -> bool {
    is_tty && no_color.is_none_or(str::is_empty)
}

fn to_text(report: &Report, color: bool) -> String {
    let body = if report.findings.is_empty() {
        "No issues found.\n".to_owned()
    } else {
        groups(&report.findings)
            .into_iter()
            .map(|service| render_group(report, service, color))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "{body}\n{}\n",
        summary_line(report.summary(), report.ignored.len())
    )
}

/// Services in order of first appearance, the project group (`None`) first.
fn groups(findings: &[Finding]) -> Vec<Option<&str>> {
    let mut services = findings
        .iter()
        .map(|finding| finding.service.as_deref())
        .fold(Vec::new(), |mut seen, service| {
            if !seen.contains(&service) {
                seen.push(service);
            }
            seen
        });
    services.sort_by_key(Option::is_some);
    services
}

fn render_group(report: &Report, service: Option<&str>, color: bool) -> String {
    let header = paint(service.unwrap_or(PROJECT_GROUP), BOLD, color);
    report
        .findings
        .iter()
        .filter(|finding| finding.service.as_deref() == service)
        .map(|finding| render_finding(&report.files, finding, color))
        .fold(format!("{header}\n"), |text, finding| text + &finding)
}

fn render_finding(files: &[String], finding: &Finding, color: bool) -> String {
    let location = finding.location;
    let severity = format!("{:<7}", finding.severity.as_str());
    format!(
        "  {} {} {}:{}:{}\n{DETAIL_INDENT}{}\n{DETAIL_INDENT}{} {}\n",
        paint(&severity, severity_style(finding.severity), color),
        paint(finding.rule.as_str(), BOLD, color),
        file_name(files, location.file),
        location.line,
        location.column,
        finding.message,
        paint("fix:", DIM, color),
        finding.fix,
    )
}

fn summary_line(summary: Summary, ignored: usize) -> String {
    let counts = format!(
        "{}, {}, {} info",
        plural(summary.error, "error", "errors"),
        plural(summary.warning, "warning", "warnings"),
        summary.info,
    );
    match ignored {
        0 => counts,
        count => format!(
            "{counts}, {} ignored by configuration",
            plural(count, "finding", "findings")
        ),
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

const fn severity_style(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => RED,
        Severity::Warning => YELLOW,
        Severity::Info => BLUE,
    }
}

fn paint(text: &str, style: &str, color: bool) -> String {
    if color {
        format!("{style}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_report;
    use eyeship_domain::{Finding, Severity};

    fn text(report: &Report, color: bool) -> String {
        let mut out = Vec::new();
        render_text(report, color, &mut out).expect("writing to a Vec never fails");
        String::from_utf8(out).expect("output is UTF-8")
    }

    #[test]
    fn renders_findings_grouped_by_service() {
        insta::assert_snapshot!(text(&sample_report(), false));
    }

    #[test]
    fn renders_an_empty_report() {
        insta::assert_snapshot!(text(&Report::default(), false));
    }

    #[test]
    fn project_findings_come_first_then_services_in_order_of_appearance() {
        let output = text(&sample_report(), false);
        let position = |header: &str| output.find(header).expect("header is present");
        assert!(position("(project)") < position("api\n"));
        assert!(position("api\n") < position("db\n"));
    }

    #[test]
    fn summary_uses_singular_and_plural_forms() {
        let base = sample_report();
        let report = Report {
            findings: base.findings[2..3].to_vec(),
            ignored: base.ignored[..1].to_vec(),
            ..base
        };
        assert!(
            text(&report, false)
                .ends_with("0 errors, 1 warning, 0 info, 1 finding ignored by configuration\n")
        );
    }

    #[test]
    fn summary_omits_ignored_count_when_nothing_is_ignored() {
        let report = Report {
            ignored: Vec::new(),
            ..sample_report()
        };
        assert!(text(&report, false).ends_with("2 errors, 1 warning, 1 info\n"));
    }

    #[test]
    fn an_unknown_file_is_shown_as_such() {
        let base = sample_report();
        let report = Report {
            files: Vec::new(),
            findings: vec![Finding {
                severity: Severity::Info,
                ..base.findings[0].clone()
            }],
            ..base
        };
        assert!(text(&report, false).contains("<unknown>:4:5"));
    }

    #[test]
    fn colored_output_contains_ansi_escape_codes() {
        let output = text(&sample_report(), true);
        assert!(output.contains("\x1b[1;31merror"));
        assert!(output.contains("\x1b[0m"));
    }

    #[test]
    fn plain_output_has_no_escape_codes() {
        assert!(!text(&sample_report(), false).contains('\x1b'));
    }

    #[test]
    fn colors_only_on_a_terminal_without_no_color() {
        assert!(should_color(true, None));
        assert!(should_color(true, Some("")));
        assert!(!should_color(true, Some("1")));
        assert!(!should_color(false, None));
    }
}
