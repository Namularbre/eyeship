//! R004: inconsistent healthcheck.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{Healthcheck, Project, Service};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R004"),
    title: "Inconsistent healthcheck",
    severity: Severity::Warning,
    doc: RuleDoc {
        scenario: "A `timeout` greater than or equal to `interval` makes checks overlap and \
                   flags the service unhealthy by mistake; `retries: 0` means the service is \
                   never reliably detected as unhealthy.",
        fix: "Keep `timeout` strictly below `interval` and set `retries` to at least 1 (3 is common).",
        bad_example: "services:
  api:
    image: acme/api:1.4.2
    healthcheck:
      test: [\"CMD\", \"curl\", \"-f\", \"http://localhost:8080/health\"]
      interval: 10s
      timeout: 30s
      retries: 0
",
        good_example: "services:
  api:
    image: acme/api:1.4.2
    healthcheck:
      test: [\"CMD\", \"curl\", \"-f\", \"http://localhost:8080/health\"]
      interval: 30s
      timeout: 5s
      retries: 3
",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .filter_map(|service| {
            service
                .healthcheck
                .as_ref()
                .filter(|healthcheck| !healthcheck.disabled)
                .map(|healthcheck| (service, healthcheck))
        })
        .flat_map(|(service, healthcheck)| {
            overlapping_timeout(service, healthcheck)
                .into_iter()
                .chain(zero_retries(service, healthcheck))
        })
        .collect()
}

/// `timeout >= interval`, only when both are explicitly set to non-zero
/// values (Docker treats zero as "use the default").
fn overlapping_timeout(service: &Service, healthcheck: &Healthcheck) -> Option<Finding> {
    let interval = healthcheck.interval.as_ref()?;
    let timeout = healthcheck.timeout.as_ref()?;
    let both_set = !interval.value.is_zero() && !timeout.value.is_zero();
    (both_set && timeout.value >= interval.value).then(|| {
        RULE.finding(
            Some(&service.name),
            timeout.location,
            format!(
                "healthcheck timeout {:?} is not shorter than its interval {:?}",
                timeout.value, interval.value
            ),
            "Set the healthcheck `timeout` strictly below its `interval`.",
        )
    })
}

fn zero_retries(service: &Service, healthcheck: &Healthcheck) -> Option<Finding> {
    healthcheck
        .retries
        .as_ref()
        .filter(|retries| retries.value == 0)
        .map(|retries| {
            RULE.finding(
                Some(&service.name),
                retries.location,
                "healthcheck retries is 0",
                "Set the healthcheck `retries` to at least 1 (3 is common).",
            )
        })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::test_support::{at, located, project, seconds, service};

    fn healthcheck(
        interval: Option<u64>,
        timeout: Option<u64>,
        retries: Option<u32>,
    ) -> Healthcheck {
        Healthcheck {
            disabled: false,
            interval: interval.map(|value| located(seconds(value), 5)),
            timeout: timeout.map(|value| located(seconds(value), 6)),
            retries: retries.map(|value| located(value, 7)),
            location: at(4),
        }
    }

    fn api(healthcheck: Healthcheck) -> Service {
        Service {
            healthcheck: Some(healthcheck),
            ..service("api", 2)
        }
    }

    fn lines(healthcheck: Healthcheck) -> Vec<u32> {
        check(&project(vec![api(healthcheck)]))
            .iter()
            .map(|finding| finding.location.line)
            .collect()
    }

    #[test]
    fn warns_on_zero_retries() {
        let findings = check(&project(vec![api(healthcheck(None, None, Some(0)))]));
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.rule, RuleId("R004"));
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(finding.service.as_deref(), Some("api"));
        assert_eq!(finding.location, at(7));
    }

    #[test]
    fn warns_when_timeout_exceeds_interval() {
        let findings = check(&project(vec![api(healthcheck(Some(10), Some(30), None))]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location, at(6));
        assert!(
            findings[0].message.contains("30s"),
            "{}",
            findings[0].message
        );
        assert!(
            findings[0].message.contains("10s"),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn warns_when_timeout_equals_interval() {
        assert_eq!(lines(healthcheck(Some(10), Some(10), Some(3))), [6]);
    }

    #[test]
    fn reports_both_problems_of_one_healthcheck() {
        assert_eq!(lines(healthcheck(Some(10), Some(30), Some(0))), [6, 7]);
    }

    #[test]
    fn accepts_a_consistent_healthcheck() {
        assert!(lines(healthcheck(Some(30), Some(5), Some(3))).is_empty());
    }

    #[test]
    fn does_not_compare_against_compose_defaults() {
        // A 60s timeout without interval exceeds the 30s default: stay silent.
        assert!(lines(healthcheck(None, Some(60), None)).is_empty());
        assert!(lines(healthcheck(Some(1), None, None)).is_empty());
    }

    #[test]
    fn treats_zero_durations_as_defaults() {
        assert!(lines(healthcheck(Some(0), Some(5), None)).is_empty());
        assert!(lines(healthcheck(Some(10), Some(0), None)).is_empty());
    }

    #[test]
    fn compares_sub_second_durations() {
        let hc = Healthcheck {
            interval: Some(located(Duration::from_millis(500), 5)),
            timeout: Some(located(Duration::from_millis(1500), 6)),
            ..healthcheck(None, None, None)
        };
        assert_eq!(lines(hc), [6]);
    }

    #[test]
    fn ignores_a_disabled_healthcheck() {
        let hc = Healthcheck {
            disabled: true,
            ..healthcheck(Some(10), Some(30), Some(0))
        };
        assert!(lines(hc).is_empty());
    }

    #[test]
    fn stays_silent_without_healthcheck() {
        assert!(check(&project(vec![service("api", 2)])).is_empty());
    }
}
