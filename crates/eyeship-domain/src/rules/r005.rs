//! R005: a service without a restart policy stays dead after a crash.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{Condition, Project, RestartPolicy, Service};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R005"),
    title: "No restart policy",
    severity: Severity::Warning,
    doc: RuleDoc {
        scenario: "Without a restart policy (or with `restart: \"no\"`), a single crash leaves the \
                   service down until someone restarts it by hand, and it does not come back after \
                   a host reboot. One-shot jobs awaited with `service_completed_successfully` are \
                   exempt.",
        fix: "Set `restart: unless-stopped` (or `always`, or `on-failure` for jobs that may fail).",
        bad_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    restart: unless-stopped\n",
    },
    check,
};

const FIX: &str = "Add `restart: unless-stopped` to the service.";

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .filter(|service| !is_one_shot_job(project, service))
        .filter_map(|service| match &service.restart {
            None => Some(RULE.finding(
                Some(&service.name),
                service.location,
                "service has no restart policy and stays down after a crash",
                FIX,
            )),
            Some(restart) if restart.value == RestartPolicy::No => Some(RULE.finding(
                Some(&service.name),
                restart.location,
                "restart policy 'no' leaves the service down after a crash",
                FIX,
            )),
            Some(_) => None,
        })
        .collect()
}

/// A service another one waits for with `service_completed_successfully`
/// is expected to exit: restarting it would be wrong.
fn is_one_shot_job(project: &Project, service: &Service) -> bool {
    project
        .services
        .iter()
        .flat_map(|other| &other.depends_on)
        .any(|dependency| {
            dependency.service == service.name
                && dependency.condition == Condition::CompletedSuccessfully
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{at, depends_on, located, project, service};

    fn with_restart(name: &str, policy: RestartPolicy) -> Service {
        Service {
            restart: Some(located(policy, 4)),
            ..service(name, 2)
        }
    }

    #[test]
    fn flags_a_missing_restart_policy_on_the_service() {
        let findings = check(&project(vec![service("api", 2)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RuleId("R005"));
        assert_eq!(findings[0].service.as_deref(), Some("api"));
        assert_eq!(findings[0].location, at(2));
    }

    #[test]
    fn flags_restart_no_on_the_value() {
        let findings = check(&project(vec![with_restart("api", RestartPolicy::No)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location, at(4));
        assert!(findings[0].message.contains("'no'"));
    }

    #[test]
    fn accepts_real_restart_policies() {
        let services = [
            RestartPolicy::Always,
            RestartPolicy::OnFailure,
            RestartPolicy::UnlessStopped,
        ]
        .into_iter()
        .map(|policy| with_restart("api", policy))
        .collect();
        assert!(check(&project(services)).is_empty());
    }

    #[test]
    fn stays_silent_on_unknown_policies() {
        let services = vec![with_restart(
            "api",
            RestartPolicy::Unknown("sometimes".into()),
        )];
        assert!(check(&project(services)).is_empty());
    }

    #[test]
    fn exempts_one_shot_jobs_awaited_until_completion() {
        let api = Service {
            restart: Some(located(RestartPolicy::Always, 9)),
            depends_on: vec![depends_on("migrate", Condition::CompletedSuccessfully, 10)],
            ..service("api", 8)
        };
        assert!(check(&project(vec![service("migrate", 2), api])).is_empty());
    }

    #[test]
    fn exempts_one_shot_jobs_with_restart_no() {
        let api = Service {
            restart: Some(located(RestartPolicy::Always, 9)),
            depends_on: vec![depends_on("migrate", Condition::CompletedSuccessfully, 10)],
            ..service("api", 8)
        };
        let migrate = with_restart("migrate", RestartPolicy::No);
        assert!(check(&project(vec![migrate, api])).is_empty());
    }

    #[test]
    fn other_conditions_do_not_exempt_the_dependency() {
        let api = Service {
            restart: Some(located(RestartPolicy::Always, 9)),
            depends_on: vec![
                depends_on("db", Condition::Healthy, 10),
                depends_on("cache", Condition::Started, 11),
            ],
            ..service("api", 8)
        };
        let findings = check(&project(vec![service("db", 2), service("cache", 5), api]));
        let names: Vec<_> = findings
            .iter()
            .map(|finding| finding.service.as_deref())
            .collect();
        assert_eq!(names, [Some("db"), Some("cache")]);
    }

    #[test]
    fn the_exemption_only_applies_to_the_awaited_service() {
        let api = Service {
            depends_on: vec![depends_on("migrate", Condition::CompletedSuccessfully, 10)],
            ..service("api", 8)
        };
        let findings = check(&project(vec![service("migrate", 2), api]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].service.as_deref(), Some("api"));
    }

    #[test]
    fn an_empty_project_has_no_findings() {
        assert!(check(&project(vec![])).is_empty());
    }
}
