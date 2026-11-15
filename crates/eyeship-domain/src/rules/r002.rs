//! R002: `depends_on` with `service_healthy` towards a service without healthcheck.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{Condition, Project};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R002"),
    title: "depends_on towards a service without healthcheck",
    severity: Severity::Error,
    doc: RuleDoc {
        scenario: "`condition: service_healthy` towards a service that has no healthcheck \
                   (or whose healthcheck is disabled): the dependency never becomes healthy \
                   and the deployment fails.",
        fix: "Add a healthcheck to the dependency, or remove `disable: true` from it.",
        bad_example: "services:
  api:
    image: acme/api:1.4.2
    depends_on:
      db:
        condition: service_healthy
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
                .filter(|dep| dep.condition == Condition::Healthy)
                .filter_map(move |dep| {
                    // A missing target is reported by R003.
                    let target = project.service(&dep.service)?;
                    let problem = match &target.healthcheck {
                        None => "has no healthcheck",
                        Some(healthcheck) if healthcheck.disabled => "has its healthcheck disabled",
                        Some(_) => return None,
                    };
                    Some(RULE.finding(
                        Some(&service.name),
                        dep.location,
                        format!(
                            "depends_on '{}' waits for it to be healthy, but '{}' {problem}",
                            dep.service, dep.service
                        ),
                        format!("Add an enabled healthcheck to service '{}'.", dep.service),
                    ))
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Healthcheck, Service};
    use crate::test_support::{at, depends_on, project, service};

    fn api_depending_on(condition: Condition) -> Service {
        Service {
            depends_on: vec![depends_on("db", condition, 5)],
            ..service("api", 2)
        }
    }

    fn healthcheck(disabled: bool) -> Option<Healthcheck> {
        Some(Healthcheck {
            disabled,
            interval: None,
            timeout: None,
            retries: None,
            location: at(9),
        })
    }

    fn db(healthcheck: Option<Healthcheck>) -> Service {
        Service {
            healthcheck,
            ..service("db", 8)
        }
    }

    #[test]
    fn fails_when_the_dependency_has_no_healthcheck() {
        let findings = check(&project(vec![
            api_depending_on(Condition::Healthy),
            db(None),
        ]));
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.rule, RuleId("R002"));
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.service.as_deref(), Some("api"));
        assert_eq!(finding.location, at(5));
        assert!(finding.message.contains("'db'"), "{}", finding.message);
    }

    #[test]
    fn fails_when_the_dependency_healthcheck_is_disabled() {
        let findings = check(&project(vec![
            api_depending_on(Condition::Healthy),
            db(healthcheck(true)),
        ]));
        assert_eq!(findings.len(), 1);
        assert!(
            findings[0].message.contains("disabled"),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn accepts_a_dependency_with_a_healthcheck() {
        let findings = check(&project(vec![
            api_depending_on(Condition::Healthy),
            db(healthcheck(false)),
        ]));
        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_dependencies_that_do_not_wait_for_health() {
        let findings = check(&project(vec![
            Service {
                depends_on: vec![
                    depends_on("db", Condition::Started, 5),
                    depends_on("db", Condition::CompletedSuccessfully, 6),
                ],
                ..service("api", 2)
            },
            db(None),
        ]));
        assert!(findings.is_empty());
    }

    #[test]
    fn stays_silent_on_a_missing_service_left_to_r003() {
        let findings = check(&project(vec![api_depending_on(Condition::Healthy)]));
        assert!(findings.is_empty());
    }
}
