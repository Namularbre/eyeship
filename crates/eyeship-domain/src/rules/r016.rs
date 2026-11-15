//! R016: `container_name` not in kebab-case.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::Project;
use crate::rule::{Rule, RuleDoc};

use super::r014::is_valid_container_name;

pub const RULE: Rule = Rule {
    id: RuleId("R016"),
    title: "Container name not in kebab-case",
    severity: Severity::Info,
    doc: RuleDoc {
        scenario: "A `container_name` breaks the kebab-case naming convention (`my-service`): \
                   names become inconsistent across projects and harder to search in logs and \
                   monitoring. Style rule, can be disabled.",
        fix: "Rename the container in kebab-case: lowercase letters and digits separated by \
              single hyphens.",
        bad_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    container_name: Acme_API\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    container_name: acme-api\n",
    },
    check,
};

/// Whether `name` matches `[a-z0-9]+(-[a-z0-9]+)*`.
fn is_kebab_case(name: &str) -> bool {
    name.split('-').all(|word| {
        !word.is_empty()
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .filter_map(|service| {
            service
                .container_name
                .as_ref()
                .filter(|name| is_valid_container_name(&name.value) && !is_kebab_case(&name.value))
                .map(|name| {
                    RULE.finding(
                        Some(&service.name),
                        name.location,
                        format!("container_name '{}' is not in kebab-case", name.value),
                        format!(
                            "rename it in kebab-case, e.g. '{}'",
                            to_kebab_case(&name.value)
                        ),
                    )
                })
        })
        .collect()
}

/// A kebab-case suggestion: lowercase, other separators turned into single hyphens.
fn to_kebab_case(name: &str) -> String {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Service;
    use crate::test_support::{at, located, project, service};

    fn named(name: &str) -> Service {
        Service {
            container_name: Some(located(name.to_owned(), 4)),
            ..service("api", 2)
        }
    }

    #[test]
    fn reports_a_name_with_uppercase_and_underscore() {
        let findings = (RULE.check)(&project(vec![named("Acme_API")]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].service.as_deref(), Some("api"));
        assert_eq!(findings[0].location, at(4));
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].message.contains("'Acme_API'"));
        assert!(findings[0].fix.contains("'acme-api'"));
    }

    #[test]
    fn reports_valid_docker_names_that_are_not_kebab_case() {
        [
            "acme_api",
            "acmeApi",
            "acme.api",
            "acme--api",
            "acme-",
            "1-API",
        ]
        .iter()
        .for_each(|name| {
            assert_eq!((RULE.check)(&project(vec![named(name)])).len(), 1, "{name}");
        });
    }

    #[test]
    fn accepts_kebab_case_names() {
        ["api", "acme-api", "acme-api-2", "0db", "a-b-c"]
            .iter()
            .for_each(|name| {
                assert!(
                    (RULE.check)(&project(vec![named(name)])).is_empty(),
                    "{name}"
                );
            });
    }

    #[test]
    fn stays_silent_on_names_r014_already_reports() {
        ["my api", "-api", "api/1", "café"].iter().for_each(|name| {
            assert!(
                (RULE.check)(&project(vec![named(name)])).is_empty(),
                "{name}"
            );
        });
    }

    #[test]
    fn stays_silent_on_an_empty_name() {
        assert!((RULE.check)(&project(vec![named("")])).is_empty());
    }

    #[test]
    fn stays_silent_without_container_name() {
        assert!((RULE.check)(&project(vec![service("api", 2)])).is_empty());
    }
}
