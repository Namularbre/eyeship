//! R007: json-file logs without rotation fill the disk.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{Logging, Project};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R007"),
    title: "Logs without rotation",
    severity: Severity::Warning,
    doc: RuleDoc {
        scenario: "The default `json-file` logging driver keeps every log line forever unless \
                   `max-size` is set. A chatty service slowly fills the disk until the host, the \
                   database and every container on it fail to write.",
        fix: "Set `logging.options.max-size` (and `max-file`) for the `json-file` driver, or use \
              a driver that rotates by itself such as `local`.",
        bad_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n",
        good_example: "services:\n  api:\n    image: ghcr.io/acme/api:1.4.2\n    logging:\n      driver: json-file\n      options:\n        max-size: 10m\n        max-file: \"3\"\n",
    },
    check,
};

const FIX: &str = "Add `logging: {driver: json-file, options: {max-size: 10m, max-file: \"3\"}}` \
                   to the service.";

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .filter_map(|service| match &service.logging {
            None => Some(RULE.finding(
                Some(&service.name),
                service.location,
                "service uses the default json-file log driver without rotation",
                FIX,
            )),
            Some(logging) if lacks_rotation(logging) => Some(RULE.finding(
                Some(&service.name),
                logging.location,
                "json-file logging has no 'max-size' and grows without limit",
                FIX,
            )),
            Some(_) => None,
        })
        .collect()
}

/// Only the `json-file` driver (explicit or default) needs `max-size`.
fn lacks_rotation(logging: &Logging) -> bool {
    logging
        .driver
        .as_deref()
        .is_none_or(|driver| driver == "json-file")
        && logging.max_size.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Service;
    use crate::test_support::{at, project, service};

    fn with_logging(driver: Option<&str>, max_size: Option<&str>) -> Service {
        Service {
            logging: Some(Logging {
                driver: driver.map(str::to_owned),
                max_size: max_size.map(str::to_owned),
                location: at(4),
            }),
            ..service("api", 2)
        }
    }

    #[test]
    fn flags_a_service_without_logging_on_the_service() {
        let findings = check(&project(vec![service("api", 2)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RuleId("R007"));
        assert_eq!(findings[0].service.as_deref(), Some("api"));
        assert_eq!(findings[0].location, at(2));
    }

    #[test]
    fn flags_explicit_json_file_without_max_size_on_the_logging_block() {
        let findings = check(&project(vec![with_logging(Some("json-file"), None)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location, at(4));
    }

    #[test]
    fn flags_a_logging_block_without_driver_nor_max_size() {
        let findings = check(&project(vec![with_logging(None, None)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location, at(4));
    }

    #[test]
    fn accepts_json_file_with_max_size() {
        let services = vec![
            with_logging(Some("json-file"), Some("10m")),
            with_logging(None, Some("10m")),
        ];
        assert!(check(&project(services)).is_empty());
    }

    #[test]
    fn accepts_drivers_that_do_not_write_unbounded_files() {
        let services = ["local", "syslog", "journald", "none", "fluentd", "awslogs"]
            .into_iter()
            .map(|driver| with_logging(Some(driver), None))
            .collect();
        assert!(check(&project(services)).is_empty());
    }

    #[test]
    fn an_empty_project_has_no_findings() {
        assert!(check(&project(vec![])).is_empty());
    }
}
