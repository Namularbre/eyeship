//! Rules: pure functions from a project to findings, with their documentation.

use crate::finding::{Finding, Location, RuleId, Severity};
use crate::model::Project;

/// Documentation of a rule, shown by `eyeship explain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleDoc {
    /// The production failure the rule prevents.
    pub scenario: &'static str,
    /// How to fix it.
    pub fix: &'static str,
    /// A faulty example.
    pub bad_example: &'static str,
    /// The corrected example.
    pub good_example: &'static str,
}

/// A reliability rule.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub id: RuleId,
    pub title: &'static str,
    /// Default severity of the findings.
    pub severity: Severity,
    pub doc: RuleDoc,
    pub check: fn(&Project) -> Vec<Finding>,
}

impl Rule {
    /// Builds a finding of this rule with its default severity.
    pub fn finding(
        &self,
        service: Option<&str>,
        location: Location,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Finding {
        Finding {
            rule: self.id,
            severity: self.severity,
            service: service.map(str::to_owned),
            location,
            message: message.into(),
            fix: fix.into(),
        }
    }
}
