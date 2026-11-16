//! R013: environment variable referenced without default and never defined.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::Project;
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R013"),
    title: "Undefined environment variable",
    severity: Severity::Error,
    doc: RuleDoc {
        scenario: "A `${VAR}` reference has no default value and the variable is absent from both \
                   the environment and the `.env` file: Compose replaces it with an empty string \
                   and the application starts misconfigured.",
        fix: "Define the variable in the environment or in `.env`, or give it a default with \
              `${VAR:-default}`, or make Compose fail loudly with `${VAR:?message}`.",
        bad_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    environment:\n      LOG_LEVEL: ${LOG_LEVEL}\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    environment:\n      LOG_LEVEL: ${LOG_LEVEL:-info}\n",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    project
        .unresolved_vars
        .iter()
        .map(|var| {
            RULE.finding(
                var.service.as_deref(),
                var.location,
                format!("variable '{}' is not set and has no default", var.name),
                format!(
                    "define {name} in the environment or in .env, or use ${{{name}:-default}} or ${{{name}:?message}}",
                    name = var.name
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::UnresolvedVar;
    use crate::test_support::{at, project, service};

    fn unresolved(name: &str, service: Option<&str>, line: u32) -> UnresolvedVar {
        UnresolvedVar {
            name: name.into(),
            service: service.map(str::to_owned),
            location: at(line),
        }
    }

    #[test]
    fn reports_each_unresolved_variable_with_its_service() {
        let project = crate::model::Project {
            unresolved_vars: vec![
                unresolved("DB_PASSWORD", Some("api"), 7),
                unresolved("LOG_LEVEL", Some("worker"), 12),
            ],
            ..project(vec![service("api", 2), service("worker", 10)])
        };
        let findings = (RULE.check)(&project);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].service.as_deref(), Some("api"));
        assert_eq!(findings[0].location, at(7));
        assert_eq!(
            findings[0].message,
            "variable 'DB_PASSWORD' is not set and has no default"
        );
        assert!(findings[0].fix.contains("${DB_PASSWORD:-default}"));
        assert_eq!(findings[1].service.as_deref(), Some("worker"));
        assert_eq!(findings[1].severity, Severity::Error);
    }

    #[test]
    fn reports_variables_outside_services_without_service() {
        let project = crate::model::Project {
            unresolved_vars: vec![unresolved("NET_NAME", None, 20)],
            ..project(vec![service("api", 2)])
        };
        let findings = (RULE.check)(&project);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].service, None);
        assert_eq!(findings[0].location, at(20));
    }

    #[test]
    fn reports_the_same_variable_once_per_occurrence() {
        let project = crate::model::Project {
            unresolved_vars: vec![
                unresolved("TAG", Some("api"), 3),
                unresolved("TAG", Some("worker"), 8),
            ],
            ..project(vec![service("api", 2), service("worker", 7)])
        };
        assert_eq!((RULE.check)(&project).len(), 2);
    }

    #[test]
    fn stays_silent_when_every_variable_resolves() {
        let project = project(vec![service("api", 2)]);
        assert!((RULE.check)(&project).is_empty());
    }

    #[test]
    fn stays_silent_on_an_empty_project() {
        assert!((RULE.check)(&crate::model::Project::default()).is_empty());
    }
}
