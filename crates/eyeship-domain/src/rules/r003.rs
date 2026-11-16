//! R003: invalid dependency (missing service or dependency cycle).

use std::collections::{BTreeMap, BTreeSet};

use crate::finding::{Finding, RuleId, Severity};
use crate::model::Project;
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R003"),
    title: "Invalid dependency",
    severity: Severity::Error,
    doc: RuleDoc {
        scenario: "`depends_on` names a service that does not exist, or services depend on \
                   each other in a cycle: Compose refuses to start the stack.",
        fix: "Point `depends_on` at an existing service and break any dependency cycle.",
        bad_example: "services:
  api:
    image: acme/api:1.4.2
    depends_on:
      - database
      - worker
  worker:
    image: acme/worker:1.4.2
    depends_on:
      - api
  db:
    image: postgres:16.4
",
        good_example: "services:
  api:
    image: acme/api:1.4.2
    depends_on:
      - db
  worker:
    image: acme/worker:1.4.2
    depends_on:
      - api
  db:
    image: postgres:16.4
",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    missing_services(project).chain(cycles(project)).collect()
}

fn missing_services(project: &Project) -> impl Iterator<Item = Finding> + '_ {
    project.services.iter().flat_map(move |service| {
        service
            .depends_on
            .iter()
            .filter(|dep| project.service(&dep.service).is_none())
            .map(move |dep| {
                RULE.finding(
                    Some(&service.name),
                    dep.location,
                    format!(
                        "depends_on '{}' names a service that does not exist",
                        dep.service
                    ),
                    format!(
                        "Fix the name '{}' or declare that service under `services`.",
                        dep.service
                    ),
                )
            })
    })
}

/// Dependency graph between existing services, with sorted, unique edges.
type Graph<'a> = BTreeMap<&'a str, BTreeSet<&'a str>>;

fn graph(project: &Project) -> Graph<'_> {
    project
        .services
        .iter()
        .map(|service| {
            let targets = service
                .depends_on
                .iter()
                .map(|dep| dep.service.as_str())
                .filter(|target| project.service(target).is_some())
                .collect();
            (service.name.as_str(), targets)
        })
        .collect()
}

/// One finding per elementary cycle. Each cycle starts from its
/// lexicographically smallest service and is reported on the edge that
/// closes it, so the output does not depend on declaration order.
fn cycles(project: &Project) -> impl Iterator<Item = Finding> + '_ {
    let graph = graph(project);
    let found: Vec<Vec<&str>> = graph
        .keys()
        .flat_map(|start| cycles_from(&graph, start, vec![start]))
        .collect();
    found
        .into_iter()
        .filter_map(move |path| cycle_finding(project, &path))
}

/// Elementary cycles through `path[0]` extending `path`, visiting only
/// services greater than the start so that each cycle is found once.
fn cycles_from<'a>(graph: &Graph<'a>, start: &'a str, path: Vec<&'a str>) -> Vec<Vec<&'a str>> {
    let last = path.last().copied().unwrap_or(start);
    graph
        .get(last)
        .into_iter()
        .flatten()
        .flat_map(|&next| {
            if next == start {
                vec![path.clone()]
            } else if next > start && !path.contains(&next) {
                cycles_from(graph, start, [path.as_slice(), &[next]].concat())
            } else {
                Vec::new()
            }
        })
        .collect()
}

fn cycle_finding(project: &Project, path: &[&str]) -> Option<Finding> {
    let (start, last) = (path.first()?, path.last()?);
    let closing = project
        .service(last)?
        .depends_on
        .iter()
        .find(|dep| dep.service == *start)?;
    let shown = path
        .iter()
        .chain(std::iter::once(start))
        .copied()
        .collect::<Vec<_>>()
        .join(" -> ");
    Some(RULE.finding(
        Some(last),
        closing.location,
        format!("dependency cycle: {shown}"),
        "Remove one of the `depends_on` entries of the cycle.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Condition, Service};
    use crate::test_support::{at, depends_on, project, service};

    /// A service at `line` depending on `targets`, each on the following lines.
    fn svc(name: &str, line: u32, targets: &[&str]) -> Service {
        Service {
            depends_on: targets
                .iter()
                .zip(line + 1..)
                .map(|(target, dep_line)| depends_on(target, Condition::Healthy, dep_line))
                .collect(),
            ..service(name, line)
        }
    }

    fn messages(services: Vec<Service>) -> Vec<String> {
        check(&project(services))
            .into_iter()
            .map(|finding| finding.message)
            .collect()
    }

    #[test]
    fn fails_on_a_missing_service() {
        let findings = check(&project(vec![
            svc("api", 1, &["database"]),
            svc("db", 10, &[]),
        ]));
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.rule, RuleId("R003"));
        assert_eq!(finding.severity, Severity::Error);
        assert_eq!(finding.service.as_deref(), Some("api"));
        assert_eq!(finding.location, at(2));
        assert!(
            finding.message.contains("'database'"),
            "{}",
            finding.message
        );
    }

    #[test]
    fn accepts_an_acyclic_graph_of_existing_services() {
        let services = vec![
            svc("api", 1, &["db", "cache"]),
            svc("worker", 5, &["api", "db"]),
            svc("db", 10, &[]),
            svc("cache", 12, &[]),
        ];
        assert!(messages(services).is_empty());
    }

    #[test]
    fn stays_silent_without_dependencies() {
        assert!(messages(vec![svc("api", 1, &[])]).is_empty());
    }

    #[test]
    fn fails_on_a_self_dependency() {
        let findings = check(&project(vec![svc("api", 1, &["api"])]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location, at(2));
        assert!(
            findings[0].message.contains("api -> api"),
            "{}",
            findings[0].message
        );
    }

    #[test]
    fn reports_a_cycle_once_on_the_edge_that_closes_it() {
        // Declared out of order on purpose: the cycle starts from `a` anyway.
        let findings = check(&project(vec![
            svc("c", 1, &["a"]),
            svc("b", 5, &["c"]),
            svc("a", 10, &["b"]),
        ]));
        assert_eq!(findings.len(), 1);
        let finding = &findings[0];
        assert_eq!(finding.service.as_deref(), Some("c"));
        assert_eq!(finding.location, at(2));
        assert!(
            finding.message.contains("a -> b -> c -> a"),
            "{}",
            finding.message
        );
    }

    #[test]
    fn reports_each_distinct_cycle() {
        let found = messages(vec![
            svc("a", 1, &["b", "c"]),
            svc("b", 5, &["a"]),
            svc("c", 10, &["a"]),
        ]);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().any(|m| m.contains("a -> b -> a")), "{found:?}");
        assert!(found.iter().any(|m| m.contains("a -> c -> a")), "{found:?}");
    }

    #[test]
    fn duplicate_edges_do_not_duplicate_a_cycle() {
        let found = messages(vec![svc("a", 1, &["b", "b"]), svc("b", 5, &["a"])]);
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_cycle_and_a_missing_service_are_both_reported() {
        let found = messages(vec![svc("a", 1, &["b", "ghost"]), svc("b", 5, &["a"])]);
        assert_eq!(found.len(), 2, "{found:?}");
    }

    #[test]
    fn output_is_deterministic() {
        let services = vec![
            svc("x", 1, &["y", "z"]),
            svc("y", 5, &["z", "x"]),
            svc("z", 10, &["x", "y"]),
        ];
        let first = messages(services.clone());
        assert_eq!(first, messages(services));
        // x->y->x, x->y->z->x, x->z->x, x->z->y->x, y->z->y
        assert_eq!(first.len(), 5, "{first:?}");
    }
}
