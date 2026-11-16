//! R006: a service without a memory limit can exhaust the host.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::Project;
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R006"),
    title: "No memory limit",
    severity: Severity::Warning,
    doc: RuleDoc {
        scenario: "Without `mem_limit` or `deploy.resources.limits.memory`, a memory leak in one \
                   container fills the host RAM, and the kernel OOM killer may kill other services \
                   (often the database) instead of the leaking one.",
        fix: "Set `mem_limit` (or `deploy.resources.limits.memory`) to a value above the normal \
              peak usage of the service.",
        bad_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    mem_limit: 512m\n",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .filter(|service| service.memory_limit.is_none())
        .map(|service| {
            RULE.finding(
                Some(&service.name),
                service.location,
                "service has no memory limit and can exhaust the host memory",
                "Add `mem_limit: 512m` (or `deploy.resources.limits.memory`) sized above the \
                 service's normal peak usage.",
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Service;
    use crate::test_support::{at, located, project, service};

    #[test]
    fn flags_a_service_without_memory_limit() {
        let findings = check(&project(vec![service("api", 2)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RuleId("R006"));
        assert_eq!(findings[0].service.as_deref(), Some("api"));
        assert_eq!(findings[0].location, at(2));
    }

    #[test]
    fn accepts_a_service_with_a_memory_limit() {
        let api = Service {
            memory_limit: Some(located(512 * 1024 * 1024, 4)),
            ..service("api", 2)
        };
        assert!(check(&project(vec![api])).is_empty());
    }

    #[test]
    fn reports_each_unlimited_service_only() {
        let limited = Service {
            memory_limit: Some(located(256 * 1024 * 1024, 4)),
            ..service("db", 2)
        };
        let findings = check(&project(vec![
            limited,
            service("api", 6),
            service("web", 9),
        ]));
        let names: Vec<_> = findings
            .iter()
            .map(|finding| finding.service.as_deref())
            .collect();
        assert_eq!(names, [Some("api"), Some("web")]);
    }

    #[test]
    fn an_empty_project_has_no_findings() {
        assert!(check(&project(vec![])).is_empty());
    }
}
