//! R010: a service uses a network that is not declared.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::Project;
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R010"),
    title: "Undeclared network",
    severity: Severity::Error,
    doc: RuleDoc {
        scenario: "A service joins a network missing from the root 'networks' section. Compose \
                   refuses the file ('service refers to undefined network') and nothing starts.",
        fix: "Declare the network under the root 'networks' key (with 'external: true' if it is \
              managed outside the project), or fix the typo in the service.",
        bad_example: "services:\n  api:\n    image: ghcr.io/acme/api:2.1.0\n    networks:\n      - backend\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:2.1.0\n    networks:\n      - backend\nnetworks:\n  backend: {}\n",
    },
    check,
};

/// The network Compose creates for every project.
const DEFAULT_NETWORK: &str = "default";

fn check(project: &Project) -> Vec<Finding> {
    let is_declared = |name: &str| {
        name == DEFAULT_NETWORK || project.networks.iter().any(|network| network.name == name)
    };
    project
        .services
        .iter()
        .flat_map(|service| {
            service
                .networks
                .iter()
                .filter(move |network| !is_declared(&network.value))
                .map(move |network| {
                    RULE.finding(
                        Some(&service.name),
                        network.location,
                        format!(
                            "network '{}' is not declared in the root 'networks' section",
                            network.value
                        ),
                        format!(
                            "Declare '{}' under the root 'networks' key (add 'external: true' \
                             if it is created outside this project), or fix the network name.",
                            network.value
                        ),
                    )
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Declared, Service};
    use crate::test_support::{declared, located, project, service};

    fn on_networks(names: &[&str]) -> Service {
        Service {
            networks: names
                .iter()
                .zip(3..)
                .map(|(name, line)| located((*name).to_owned(), line))
                .collect(),
            ..service("api", 1)
        }
    }

    fn audit(names: &[&str], networks: Vec<Declared>) -> Vec<Finding> {
        check(&Project {
            networks,
            ..project(vec![on_networks(names)])
        })
    }

    #[test]
    fn reports_an_undeclared_network_at_its_location() {
        let findings = audit(&["front", "back"], vec![declared("front", 20)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location.line, 4);
        assert_eq!(findings[0].service.as_deref(), Some("api"));
        assert!(findings[0].message.contains("'back'"));
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn reports_networks_when_no_network_is_declared() {
        assert_eq!(audit(&["front"], Vec::new()).len(), 1);
    }

    #[test]
    fn declared_networks_are_fine() {
        assert!(
            audit(
                &["front", "back"],
                vec![declared("back", 20), declared("front", 21)]
            )
            .is_empty()
        );
    }

    #[test]
    fn external_networks_count_as_declared() {
        let external = Declared {
            external: true,
            ..declared("proxy", 20)
        };
        assert!(audit(&["proxy"], vec![external]).is_empty());
    }

    #[test]
    fn the_default_network_is_always_allowed() {
        assert!(audit(&["default"], Vec::new()).is_empty());
    }

    #[test]
    fn a_service_without_networks_is_clean() {
        assert!(audit(&[], Vec::new()).is_empty());
    }

    #[test]
    fn network_names_are_case_sensitive() {
        assert_eq!(audit(&["Front"], vec![declared("front", 20)]).len(), 1);
    }
}
