//! A deployment as the interface shows it: its name and its live audit.

use std::cmp::Reverse;

use eyeship_domain::{FailOn, Report};

/// The live audit of one deployment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentResult {
    pub name: String,
    /// Audited files, or the configured ones when loading failed.
    pub files: Vec<String>,
    pub fail_on: FailOn,
    /// The report, or why the deployment could not be read.
    pub outcome: Result<Report, String>,
}

/// Whether a deployment is ready for production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Pass,
    Fail,
    Unreadable,
}

impl DeploymentResult {
    pub fn status(&self) -> Status {
        match &self.outcome {
            Err(_) => Status::Unreadable,
            Ok(report) if report.exit_code(self.fail_on) == 0 => Status::Pass,
            Ok(_) => Status::Fail,
        }
    }
}

/// Unreadable and failing deployments first, then by name (spec 004 US2 §3).
pub fn sort_deployments(mut deployments: Vec<DeploymentResult>) -> Vec<DeploymentResult> {
    deployments.sort_by(|a, b| {
        (Reverse(a.status() != Status::Pass), &a.name)
            .cmp(&(Reverse(b.status() != Status::Pass), &b.name))
    });
    deployments
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyeship_domain::{FileId, Finding, Location, RuleId, Severity};

    fn report(severity: Option<Severity>) -> Report {
        Report {
            files: vec!["compose.yaml".into()],
            findings: severity
                .map(|severity| Finding {
                    rule: RuleId("R001"),
                    severity,
                    service: Some("api".into()),
                    location: Location {
                        file: FileId(0),
                        line: 1,
                        column: 1,
                    },
                    message: "message".into(),
                    fix: "fix".into(),
                })
                .into_iter()
                .collect(),
            ignored: Vec::new(),
        }
    }

    fn deployment(name: &str, outcome: Result<Report, String>) -> DeploymentResult {
        DeploymentResult {
            name: name.into(),
            files: vec!["compose.yaml".into()],
            fail_on: FailOn::Error,
            outcome,
        }
    }

    #[test]
    fn status_follows_the_threshold() {
        assert_eq!(deployment("a", Ok(report(None))).status(), Status::Pass);
        assert_eq!(
            deployment("a", Ok(report(Some(Severity::Warning)))).status(),
            Status::Pass
        );
        assert_eq!(
            deployment("a", Ok(report(Some(Severity::Error)))).status(),
            Status::Fail
        );
        let strict = DeploymentResult {
            fail_on: FailOn::Warning,
            ..deployment("a", Ok(report(Some(Severity::Warning))))
        };
        assert_eq!(strict.status(), Status::Fail);
        assert_eq!(
            deployment("a", Err("missing".into())).status(),
            Status::Unreadable
        );
    }

    #[test]
    fn failing_deployments_come_first_then_by_name() {
        let sorted = sort_deployments(vec![
            deployment("alpha", Ok(report(None))),
            deployment("prod", Ok(report(Some(Severity::Error)))),
            deployment("broken", Err("missing".into())),
            deployment("beta", Ok(report(None))),
        ]);
        let names: Vec<&str> = sorted.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["broken", "prod", "alpha", "beta"]);
    }
}
