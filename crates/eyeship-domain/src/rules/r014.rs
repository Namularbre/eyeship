//! R014: `container_name` that Docker refuses.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::Project;
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R014"),
    title: "Invalid container name",
    severity: Severity::Error,
    doc: RuleDoc {
        scenario: "A `container_name` does not match the Docker format \
                   `[a-zA-Z0-9][a-zA-Z0-9_.-]*`: the container cannot be created and the \
                   deployment fails.",
        fix: "Use only letters, digits, `_`, `.` and `-`, starting with a letter or a digit, or \
              remove `container_name` and let Compose name the container.",
        bad_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    container_name: \"my api\"\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n",
    },
    check,
};

/// Whether Docker accepts `name` as a container name: `[a-zA-Z0-9][a-zA-Z0-9_.-]*`.
pub(crate) fn is_valid_container_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .filter_map(|service| {
            service
                .container_name
                .as_ref()
                .filter(|name| !name.value.is_empty() && !is_valid_container_name(&name.value))
                .map(|name| {
                    RULE.finding(
                        Some(&service.name),
                        name.location,
                        format!("container_name '{}' is not a valid Docker container name", name.value),
                        "use only letters, digits, '_', '.' and '-', starting with a letter or a digit, or remove container_name",
                    )
                })
        })
        .collect()
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
    fn reports_a_name_with_a_space() {
        let findings = (RULE.check)(&project(vec![named("my api")]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].service.as_deref(), Some("api"));
        assert_eq!(findings[0].location, at(4));
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].message.contains("'my api'"));
    }

    #[test]
    fn reports_names_with_a_bad_first_character() {
        ["-api", "_api", ".api", "/api"].iter().for_each(|name| {
            assert_eq!((RULE.check)(&project(vec![named(name)])).len(), 1, "{name}");
        });
    }

    #[test]
    fn reports_names_with_forbidden_characters() {
        ["api/1", "api:1", "api@x", "café", "api\t"]
            .iter()
            .for_each(|name| {
                assert_eq!((RULE.check)(&project(vec![named(name)])).len(), 1, "{name}");
            });
    }

    #[test]
    fn accepts_valid_names() {
        ["api", "API_1", "a", "0db", "my-app.web_1", "My.App"]
            .iter()
            .for_each(|name| {
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
