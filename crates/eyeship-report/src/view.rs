//! An owned, format-independent view of a report: what the HTML pages show,
//! whether the report was just computed or read back from a saved file.

use eyeship_domain::{FailOn, Finding, Report, Severity, Summary};

/// One finding, with its file name resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingView {
    pub rule: String,
    pub severity: Severity,
    pub service: Option<String>,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub fix: String,
}

/// A report whose findings own their data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReportView {
    pub files: Vec<String>,
    /// Retained findings, in the engine's order.
    pub findings: Vec<FindingView>,
    /// Findings silenced by configuration.
    pub ignored: Vec<FindingView>,
}

impl FindingView {
    fn from_finding(files: &[String], finding: &Finding) -> Self {
        Self {
            rule: finding.rule.as_str().to_owned(),
            severity: finding.severity,
            service: finding.service.clone(),
            file: crate::file_name(files, finding.location.file).to_owned(),
            line: finding.location.line,
            column: finding.location.column,
            message: finding.message.clone(),
            fix: finding.fix.clone(),
        }
    }

    /// Identity of a finding across two reports: its line may move.
    pub fn key(&self) -> (&str, Option<&str>, &str) {
        (&self.rule, self.service.as_deref(), &self.message)
    }
}

impl From<&Report> for ReportView {
    fn from(report: &Report) -> Self {
        let views = |findings: &[Finding]| {
            findings
                .iter()
                .map(|finding| FindingView::from_finding(&report.files, finding))
                .collect()
        };
        Self {
            files: report.files.clone(),
            findings: views(&report.findings),
            ignored: views(&report.ignored),
        }
    }
}

impl ReportView {
    /// Number of retained findings per severity.
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

    /// Whether a finding reaches the threshold, as `Report::exit_code` decides.
    pub fn fails(&self, fail_on: FailOn) -> bool {
        fail_on.threshold().is_some_and(|threshold| {
            self.findings
                .iter()
                .any(|finding| finding.severity >= threshold)
        })
    }

    /// Services that have at least one retained finding, sorted.
    pub fn services(&self) -> Vec<&str> {
        let mut services: Vec<&str> = self
            .findings
            .iter()
            .filter_map(|finding| finding.service.as_deref())
            .collect();
        services.sort_unstable();
        services.dedup();
        services
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_report;

    #[test]
    fn resolves_file_names_and_keeps_the_order() {
        let view = ReportView::from(&sample_report());
        let rules: Vec<&str> = view.findings.iter().map(|f| f.rule.as_str()).collect();
        assert_eq!(rules, ["R005", "R002", "R001", "R017"]);
        assert_eq!(view.findings[2].file, "compose.override.yaml");
        assert_eq!(view.ignored.len(), 2);
    }

    #[test]
    fn summary_and_threshold_match_the_domain_report() {
        let report = sample_report();
        let view = ReportView::from(&report);
        assert_eq!(view.summary(), report.summary());
        for fail_on in [FailOn::Error, FailOn::Warning, FailOn::Info, FailOn::None] {
            assert_eq!(
                view.fails(fail_on),
                report.exit_code(fail_on) == 1,
                "{fail_on:?}"
            );
        }
    }

    #[test]
    fn lists_services_with_findings() {
        assert_eq!(ReportView::from(&sample_report()).services(), ["api", "db"]);
    }

    #[test]
    fn the_key_ignores_the_position() {
        let view = ReportView::from(&sample_report());
        let moved = FindingView {
            line: 99,
            column: 1,
            ..view.findings[0].clone()
        };
        assert_eq!(moved.key(), view.findings[0].key());
    }
}
