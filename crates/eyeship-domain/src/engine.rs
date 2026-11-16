//! The audit engine: applies rules and decides the exit status.

use std::cmp::Reverse;
use std::str::FromStr;

use crate::finding::{Finding, Severity};
use crate::model::Project;
use crate::rule::Rule;

/// What the user chose to silence, explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditConfig {
    /// Rule IDs ignored everywhere.
    pub ignored_rules: Vec<String>,
    /// `(service, rule ID)` pairs ignored for one service only.
    pub ignored_for_service: Vec<(String, String)>,
}

impl AuditConfig {
    fn ignores(&self, finding: &Finding) -> bool {
        let rule = finding.rule.as_str();
        self.ignored_rules.iter().any(|id| id == rule)
            || finding.service.as_deref().is_some_and(|service| {
                self.ignored_for_service
                    .iter()
                    .any(|(name, id)| name == service && id == rule)
            })
    }
}

/// Number of findings per severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub error: usize,
    pub warning: usize,
    pub info: usize,
}

/// The result of an audit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// Source files, indexed by [`crate::FileId`].
    pub files: Vec<String>,
    /// Retained findings, in a deterministic order.
    pub findings: Vec<Finding>,
    /// Findings silenced by configuration: counted, never dropped silently.
    pub ignored: Vec<Finding>,
}

impl Report {
    pub fn summary(&self) -> Summary {
        self.findings
            .iter()
            .fold(Summary::default(), |summary, finding| {
                match finding.severity {
                    Severity::Error => Summary {
                        error: summary.error + 1,
                        ..summary
                    },
                    Severity::Warning => Summary {
                        warning: summary.warning + 1,
                        ..summary
                    },
                    Severity::Info => Summary {
                        info: summary.info + 1,
                        ..summary
                    },
                }
            })
    }

    /// `0` when no finding reaches the threshold, `1` otherwise.
    pub fn exit_code(&self, fail_on: FailOn) -> u8 {
        let fails = fail_on.threshold().is_some_and(|threshold| {
            self.findings
                .iter()
                .any(|finding| finding.severity >= threshold)
        });
        u8::from(fails)
    }
}

/// Minimum severity that makes the audit fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailOn {
    #[default]
    Error,
    Warning,
    Info,
    None,
}

impl FailOn {
    pub const fn threshold(self) -> Option<Severity> {
        match self {
            Self::Error => Some(Severity::Error),
            Self::Warning => Some(Severity::Warning),
            Self::Info => Some(Severity::Info),
            Self::None => None,
        }
    }
}

impl FromStr for FailOn {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "error" => Ok(Self::Error),
            "warning" => Ok(Self::Warning),
            "info" => Ok(Self::Info),
            "none" => Ok(Self::None),
            other => Err(format!(
                "unknown threshold '{other}' (expected error, warning, info or none)"
            )),
        }
    }
}

/// Runs every rule on the project and sorts the findings deterministically:
/// severity (most severe first), then file, line, column and rule ID.
pub fn audit(project: &Project, rules: &[Rule], config: &AuditConfig) -> Report {
    let (ignored, mut findings): (Vec<Finding>, Vec<Finding>) = rules
        .iter()
        .flat_map(|rule| (rule.check)(project))
        .partition(|finding| config.ignores(finding));
    findings.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    Report {
        files: project.files.clone(),
        findings,
        ignored,
    }
}

fn sort_key(finding: &Finding) -> impl Ord + '_ {
    (
        Reverse(finding.severity),
        finding.location,
        finding.rule,
        finding.service.as_deref(),
        finding.message.as_str(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{FileId, Location, RuleId};
    use crate::rule::RuleDoc;

    const DOC: RuleDoc = RuleDoc {
        scenario: "",
        fix: "",
        bad_example: "",
        good_example: "",
    };

    fn at(line: u32) -> Location {
        Location::new(FileId(0), line, 1)
    }

    fn warn_line_5(_: &Project) -> Vec<Finding> {
        vec![WARN.finding(Some("api"), at(5), "w", "f")]
    }

    fn error_line_9_and_info_line_1(_: &Project) -> Vec<Finding> {
        vec![
            ERROR.finding(Some("db"), at(9), "e", "f"),
            Finding {
                severity: Severity::Info,
                ..ERROR.finding(Some("db"), at(1), "i", "f")
            },
        ]
    }

    const WARN: Rule = Rule {
        id: RuleId("R002"),
        title: "warn",
        severity: Severity::Warning,
        doc: DOC,
        check: warn_line_5,
    };

    const ERROR: Rule = Rule {
        id: RuleId("R001"),
        title: "error",
        severity: Severity::Error,
        doc: DOC,
        check: error_line_9_and_info_line_1,
    };

    fn ids(findings: &[Finding]) -> Vec<(&str, u32)> {
        findings
            .iter()
            .map(|finding| (finding.rule.as_str(), finding.location.line))
            .collect()
    }

    #[test]
    fn sorts_by_severity_then_location() {
        let report = audit(&Project::default(), &[WARN, ERROR], &AuditConfig::default());
        assert_eq!(
            ids(&report.findings),
            [("R001", 9), ("R002", 5), ("R001", 1)]
        );
    }

    #[test]
    fn order_does_not_depend_on_rule_order() {
        let config = AuditConfig::default();
        let project = Project::default();
        assert_eq!(
            audit(&project, &[WARN, ERROR], &config),
            audit(&project, &[ERROR, WARN], &config)
        );
    }

    #[test]
    fn an_ignored_rule_produces_ignored_findings_only() {
        let config = AuditConfig {
            ignored_rules: vec!["R001".into()],
            ..AuditConfig::default()
        };
        let report = audit(&Project::default(), &[WARN, ERROR], &config);
        assert_eq!(ids(&report.findings), [("R002", 5)]);
        assert_eq!(report.ignored.len(), 2);
    }

    #[test]
    fn a_rule_can_be_ignored_for_one_service() {
        let config = AuditConfig {
            ignored_for_service: vec![("api".into(), "R002".into())],
            ..AuditConfig::default()
        };
        let report = audit(&Project::default(), &[WARN, ERROR], &config);
        assert_eq!(report.ignored.len(), 1);
        assert_eq!(report.findings.len(), 2);
    }

    #[test]
    fn summary_counts_retained_findings_per_severity() {
        let report = audit(&Project::default(), &[WARN, ERROR], &AuditConfig::default());
        assert_eq!(
            report.summary(),
            Summary {
                error: 1,
                warning: 1,
                info: 1
            }
        );
    }

    fn report_with(severity: Severity) -> Report {
        Report {
            findings: vec![Finding {
                severity,
                ..WARN.finding(None, at(1), "m", "f")
            }],
            ..Report::default()
        }
    }

    #[test]
    fn exit_code_follows_the_threshold() {
        let cases = [
            (Severity::Error, FailOn::Error, 1),
            (Severity::Warning, FailOn::Error, 0),
            (Severity::Warning, FailOn::Warning, 1),
            (Severity::Info, FailOn::Warning, 0),
            (Severity::Info, FailOn::Info, 1),
            (Severity::Error, FailOn::None, 0),
        ];
        for (severity, fail_on, expected) in cases {
            assert_eq!(
                report_with(severity).exit_code(fail_on),
                expected,
                "{severity} with {fail_on:?}"
            );
        }
    }

    #[test]
    fn an_empty_report_always_passes() {
        assert_eq!(Report::default().exit_code(FailOn::Info), 0);
    }

    #[test]
    fn parses_thresholds() {
        assert_eq!("warning".parse(), Ok(FailOn::Warning));
        assert!("loud".parse::<FailOn>().is_err());
    }
}
