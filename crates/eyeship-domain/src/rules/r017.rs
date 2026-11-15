//! R017: obsolete root `version` key.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::Project;
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R017"),
    title: "Obsolete version key",
    severity: Severity::Info,
    doc: RuleDoc {
        scenario: "The root `version` key is ignored by Compose v2, which prints a warning on \
                   every command: it is noise that hides real warnings and suggests a file \
                   format that no longer exists.",
        fix: "Remove the root `version` key.",
        bad_example: "version: \"3.8\"\nservices:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    project
        .has_version_key
        .map(|location| {
            RULE.finding(
                None,
                location,
                "the root 'version' key is obsolete and ignored by Compose",
                "remove the root 'version' key",
            )
        })
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Project;
    use crate::test_support::{at, project, service};

    #[test]
    fn reports_the_version_key_without_service() {
        let project = Project {
            has_version_key: Some(at(1)),
            ..project(vec![service("api", 3)])
        };
        let findings = (RULE.check)(&project);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].service, None);
        assert_eq!(findings[0].location, at(1));
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].message.contains("version"));
        assert!(findings[0].fix.contains("version"));
    }

    #[test]
    fn reports_the_version_key_even_without_services() {
        let project = Project {
            has_version_key: Some(at(1)),
            ..Project::default()
        };
        assert_eq!((RULE.check)(&project).len(), 1);
    }

    #[test]
    fn stays_silent_without_version_key() {
        assert!((RULE.check)(&project(vec![service("api", 2)])).is_empty());
    }
}
