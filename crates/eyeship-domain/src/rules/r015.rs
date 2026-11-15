//! R015: fixed `container_name`.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::Project;
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R015"),
    title: "Fixed container name",
    severity: Severity::Info,
    doc: RuleDoc {
        scenario: "`container_name` gives every container of the service the same name: the \
                   service cannot be scaled beyond one replica, and two projects deployed on the \
                   same host collide on that name.",
        fix: "Remove `container_name` and let Compose name the container \
              (`<project>-<service>-<index>`); reach the service by its service name on the \
              network.",
        bad_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    container_name: api\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .filter_map(|service| {
            service
                .container_name
                .as_ref()
                .filter(|name| !name.value.is_empty())
                .map(|name| {
                    RULE.finding(
                        Some(&service.name),
                        name.location,
                        format!(
                            "container_name '{}' prevents scaling the service and collides between projects on the same host",
                            name.value
                        ),
                        "remove container_name and reach the service by its service name",
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

    fn named(service_name: &str, container_name: &str, line: u32) -> Service {
        Service {
            container_name: Some(located(container_name.to_owned(), line + 2)),
            ..service(service_name, line)
        }
    }

    #[test]
    fn reports_a_container_name() {
        let findings = (RULE.check)(&project(vec![named("api", "api", 2)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].service.as_deref(), Some("api"));
        assert_eq!(findings[0].location, at(4));
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].message.contains("'api'"));
    }

    #[test]
    fn reports_every_service_with_a_container_name() {
        let project = project(vec![
            named("api", "acme-api", 2),
            service("cache", 6),
            named("db", "acme-db", 10),
        ]);
        let services: Vec<Option<String>> = (RULE.check)(&project)
            .into_iter()
            .map(|finding| finding.service)
            .collect();
        assert_eq!(services, [Some("api".into()), Some("db".into())]);
    }

    #[test]
    fn reports_invalid_names_too() {
        assert_eq!(
            (RULE.check)(&project(vec![named("api", "my api", 2)])).len(),
            1
        );
    }

    #[test]
    fn stays_silent_without_container_name() {
        assert!((RULE.check)(&project(vec![service("api", 2), service("db", 5)])).is_empty());
    }

    #[test]
    fn stays_silent_on_an_empty_name() {
        assert!((RULE.check)(&project(vec![named("api", "", 2)])).is_empty());
    }
}
