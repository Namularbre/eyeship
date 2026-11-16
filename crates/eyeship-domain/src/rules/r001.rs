//! R001: `depends_on` without a health condition.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{Condition, Project};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R001"),
    title: "depends_on without health condition",
    severity: Severity::Warning,
    doc: RuleDoc {
        scenario: "With the short list syntax or `condition: service_started`, the dependent \
                   service starts as soon as its dependency is launched, not when it is ready. \
                   It gets connection refused at startup and crashes.",
        fix: "Use `condition: service_healthy` and give the dependency a healthcheck.",
        bad_example: "services:
  api:
    image: acme/api:1.4.2
    depends_on:
      - db
  db:
    image: postgres:16.4
",
        good_example: "services:
  api:
    image: acme/api:1.4.2
    depends_on:
      db:
        condition: service_healthy
  db:
    image: postgres:16.4
    healthcheck:
      test: [\"CMD-SHELL\", \"pg_isready -U postgres\"]
      interval: 10s
      timeout: 5s
      retries: 5
",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .flat_map(|service| {
            service
                .depends_on
                .iter()
                .filter(|dep| dep.condition == Condition::Started)
                // A missing target is reported by R003.
                .filter(|dep| project.service(&dep.service).is_some())
                .map(move |dep| {
                    RULE.finding(
                        Some(&service.name),
                        dep.location,
                        format!(
                            "depends_on '{}' does not wait for the service to be healthy",
                            dep.service
                        ),
                        format!(
                            "Use `condition: service_healthy` for '{}' and give it a healthcheck.",
                            dep.service
                        ),
                    )
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Service;
    use crate::test_support::{at, depends_on, project, service};

    fn api_depending_on(condition: Condition) -> Service {
        Service {
            depends_on: vec![depends_on("db", condition, 5)],
            ..service("api", 2)
        }
    }

    #[test]
    fn warns_on_a_dependency_that_only_waits_for_start() {
        let findings = check(&project(vec![
            api_depending_on(Condition::Started),
            service("db", 7),
        ]));
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.rule, RuleId("R001"));
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.service.as_deref(), Some("api"));
        assert_eq!(finding.location, at(5));
        assert!(finding.message.contains("'db'"), "{}", finding.message);
    }

    #[test]
    fn reports_every_started_dependency() {
        let api = Service {
            depends_on: vec![
                depends_on("db", Condition::Started, 5),
                depends_on("cache", Condition::Healthy, 6),
                depends_on("queue", Condition::Started, 7),
            ],
            ..service("api", 2)
        };
        let findings = check(&project(vec![
            api,
            service("db", 9),
            service("cache", 11),
            service("queue", 13),
        ]));
        let lines: Vec<u32> = findings.iter().map(|f| f.location.line).collect();
        assert_eq!(lines, [5, 7]);
    }

    #[test]
    fn accepts_a_healthy_condition() {
        let findings = check(&project(vec![
            api_depending_on(Condition::Healthy),
            service("db", 7),
        ]));
        assert!(findings.is_empty());
    }

    #[test]
    fn accepts_a_completed_successfully_condition() {
        let findings = check(&project(vec![
            api_depending_on(Condition::CompletedSuccessfully),
            service("db", 7),
        ]));
        assert!(findings.is_empty());
    }

    #[test]
    fn stays_silent_without_dependencies() {
        assert!(check(&project(vec![service("api", 2)])).is_empty());
    }

    #[test]
    fn stays_silent_on_a_missing_service_left_to_r003() {
        let findings = check(&project(vec![api_depending_on(Condition::Started)]));
        assert!(findings.is_empty());
    }
}
