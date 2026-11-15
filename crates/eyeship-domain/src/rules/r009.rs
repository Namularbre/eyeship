//! R009: two published ports conflict on the host.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{PortMapping, PortRange, Project, Protocol, Service};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R009"),
    title: "Published port conflict",
    severity: Severity::Error,
    doc: RuleDoc {
        scenario: "Two containers publish the same host port with the same protocol on overlapping \
                   host IPs. Only one can bind it: the other fails to start with \
                   'port is already allocated', and the deployment is left half up.",
        fix: "Publish each host port once: pick a different host port, bind to distinct host IPs, \
              or let a single reverse proxy publish the port.",
        bad_example: "services:\n  web:\n    image: nginx:1.27.2\n    ports:\n      - \"8080:80\"\n  admin:\n    image: nginx:1.27.2\n    ports:\n      - \"8080:80\"\n",
        good_example: "services:\n  web:\n    image: nginx:1.27.2\n    ports:\n      - \"8080:80\"\n  admin:\n    image: nginx:1.27.2\n    ports:\n      - \"8081:80\"\n",
    },
    check,
};

/// A published port with the service that publishes it.
struct Published<'a> {
    service: &'a Service,
    mapping: &'a PortMapping,
    range: PortRange,
}

fn check(project: &Project) -> Vec<Finding> {
    let published: Vec<Published<'_>> = project
        .services
        .iter()
        .flat_map(|service| {
            service.ports.iter().filter_map(move |mapping| {
                mapping.published.map(|range| Published {
                    service,
                    mapping,
                    range,
                })
            })
        })
        .collect();
    published
        .iter()
        .enumerate()
        .flat_map(|(index, later)| {
            published[..index]
                .iter()
                .filter(move |earlier| conflicts(earlier, later))
                .map(move |earlier| finding(earlier, later))
        })
        .collect()
}

fn conflicts(earlier: &Published<'_>, later: &Published<'_>) -> bool {
    earlier.mapping.protocol == later.mapping.protocol
        && earlier.range.overlaps(later.range)
        && ips_overlap(
            earlier.mapping.host_ip.as_deref(),
            later.mapping.host_ip.as_deref(),
        )
        && !is_repeated_entry(earlier, later)
}

/// `None`, `0.0.0.0` and `::` bind every interface.
fn ips_overlap(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => is_wildcard(a) || is_wildcard(b) || a == b,
        _ => true,
    }
}

fn is_wildcard(ip: &str) -> bool {
    matches!(ip, "" | "0.0.0.0" | "::" | "[::]")
}

/// The exact same entry written twice in one service is not a conflict
/// between two bindings: stay silent rather than risk a false positive.
fn is_repeated_entry(earlier: &Published<'_>, later: &Published<'_>) -> bool {
    earlier.service.name == later.service.name
        && earlier.mapping.host_ip == later.mapping.host_ip
        && earlier.mapping.published == later.mapping.published
        && earlier.mapping.target == later.mapping.target
}

fn finding(earlier: &Published<'_>, later: &Published<'_>) -> Finding {
    RULE.finding(
        Some(&later.service.name),
        later.mapping.location,
        format!(
            "host port {} is already published by service '{}' ({})",
            describe(later),
            earlier.service.name,
            describe(earlier)
        ),
        format!(
            "Publish a different host port, bind to a distinct host IP, or remove one of the \
             two mappings of service '{}' and '{}'.",
            earlier.service.name, later.service.name
        ),
    )
}

fn describe(published: &Published<'_>) -> String {
    let range = if published.range.start == published.range.end {
        published.range.start.to_string()
    } else {
        format!("{}-{}", published.range.start, published.range.end)
    };
    let ip = published
        .mapping
        .host_ip
        .as_deref()
        .map_or_else(String::new, |ip| format!("{ip}:"));
    format!("{ip}{range}/{}", protocol(published.mapping.protocol))
}

const fn protocol(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Tcp => "tcp",
        Protocol::Udp => "udp",
        Protocol::Sctp => "sctp",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{port, project, service};

    fn on(ip: &str, mapping: PortMapping) -> PortMapping {
        PortMapping {
            host_ip: Some(ip.into()),
            ..mapping
        }
    }

    fn with_ports(name: &str, line: u32, ports: Vec<PortMapping>) -> Service {
        Service {
            ports,
            ..service(name, line)
        }
    }

    fn lines(findings: &[Finding]) -> Vec<(Option<&str>, u32)> {
        findings
            .iter()
            .map(|finding| (finding.service.as_deref(), finding.location.line))
            .collect()
    }

    fn two(first: PortMapping, second: PortMapping) -> Vec<Finding> {
        check(&project(vec![
            with_ports("web", 1, vec![first]),
            with_ports("admin", 5, vec![second]),
        ]))
    }

    #[test]
    fn reports_the_same_port_published_by_two_services_on_the_later_one() {
        let findings = two(port(8080, 80, 3), port(8080, 80, 7));
        assert_eq!(lines(&findings), [(Some("admin"), 7)]);
        assert!(findings[0].message.contains("8080/tcp"));
        assert!(findings[0].message.contains("'web'"));
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn reports_a_conflict_inside_one_service() {
        let findings = check(&project(vec![with_ports(
            "web",
            1,
            vec![port(8080, 80, 3), port(8080, 81, 4)],
        )]));
        assert_eq!(lines(&findings), [(Some("web"), 4)]);
    }

    #[test]
    fn reports_overlapping_ranges() {
        let range = PortMapping {
            published: Some(PortRange {
                start: 8000,
                end: 8010,
            }),
            ..port(8000, 80, 3)
        };
        let findings = two(range, port(8005, 80, 7));
        assert_eq!(lines(&findings), [(Some("admin"), 7)]);
        assert!(findings[0].message.contains("8000-8010"));
    }

    #[test]
    fn a_wildcard_ip_conflicts_with_any_ip() {
        for (first, second) in [
            (None, Some("127.0.0.1")),
            (Some("0.0.0.0"), Some("10.0.0.1")),
            (Some("::"), Some("0.0.0.0")),
            (Some("192.168.1.2"), None),
        ] {
            let bind = |ip: Option<&str>, line| PortMapping {
                host_ip: ip.map(str::to_owned),
                ..port(8080, 80, line)
            };
            assert_eq!(
                two(bind(first, 3), bind(second, 7)).len(),
                1,
                "{first:?} vs {second:?}"
            );
        }
    }

    #[test]
    fn equal_explicit_ips_conflict() {
        assert_eq!(
            two(
                on("127.0.0.1", port(80, 80, 3)),
                on("127.0.0.1", port(80, 80, 7))
            )
            .len(),
            1
        );
    }

    #[test]
    fn distinct_explicit_ips_do_not_conflict() {
        assert!(
            two(
                on("127.0.0.1", port(80, 80, 3)),
                on("10.0.0.1", port(80, 80, 7))
            )
            .is_empty()
        );
    }

    #[test]
    fn different_protocols_do_not_conflict() {
        let udp = PortMapping {
            protocol: Protocol::Udp,
            ..port(53, 53, 7)
        };
        assert!(two(port(53, 53, 3), udp).is_empty());
    }

    #[test]
    fn different_ports_and_adjacent_ranges_do_not_conflict() {
        let range = PortMapping {
            published: Some(PortRange {
                start: 8000,
                end: 8010,
            }),
            ..port(8000, 80, 3)
        };
        assert!(two(range, port(8011, 80, 7)).is_empty());
        assert!(two(port(80, 80, 3), port(443, 443, 7)).is_empty());
    }

    #[test]
    fn unpublished_ports_never_conflict() {
        let ephemeral = |line| PortMapping {
            published: None,
            ..port(80, 80, line)
        };
        assert!(two(ephemeral(3), ephemeral(7)).is_empty());
        assert!(two(port(80, 80, 3), ephemeral(7)).is_empty());
    }

    #[test]
    fn the_same_target_port_in_two_services_is_fine() {
        assert!(two(port(8080, 80, 3), port(8081, 80, 7)).is_empty());
    }

    #[test]
    fn each_conflicting_pair_is_reported_once() {
        let findings = check(&project(vec![
            with_ports("a", 1, vec![port(80, 80, 2)]),
            with_ports("b", 3, vec![port(80, 80, 4)]),
            with_ports("c", 5, vec![port(80, 80, 6)]),
        ]));
        assert_eq!(
            lines(&findings),
            [(Some("b"), 4), (Some("c"), 6), (Some("c"), 6)]
        );
        assert!(findings[1].message.contains("'a'"));
        assert!(findings[2].message.contains("'b'"));
    }

    #[test]
    fn the_same_entry_repeated_in_one_service_is_not_reported() {
        let findings = check(&project(vec![with_ports(
            "web",
            1,
            vec![port(8080, 80, 3), port(8080, 80, 4)],
        )]));
        assert!(findings.is_empty());
    }

    #[test]
    fn a_project_without_ports_is_clean() {
        assert!(check(&project(vec![service("web", 1)])).is_empty());
    }
}
