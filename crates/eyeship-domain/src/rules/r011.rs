//! R011: a named volume is not declared, or a volume entry is malformed.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{Mount, Project};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R011"),
    title: "Undeclared or malformed volume",
    severity: Severity::Error,
    doc: RuleDoc {
        scenario: "A service mounts a named volume missing from the root 'volumes' section, or \
                   writes a volume entry Compose cannot parse (bad 'source:target:mode', unknown \
                   mode). Compose rejects the file and the deployment does not start.",
        fix: "Declare every named volume under the root 'volumes' key and write entries as \
              'source:target[:ro|rw]'.",
        bad_example: "services:\n  db:\n    image: postgres:16.4\n    volumes:\n      - pgdata:/var/lib/postgresql/data\n",
        good_example: "services:\n  db:\n    image: postgres:16.4\n    volumes:\n      - pgdata:/var/lib/postgresql/data\nvolumes:\n  pgdata: {}\n",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    let is_declared = |name: &str| project.volumes.iter().any(|volume| volume.name == name);
    project
        .services
        .iter()
        .flat_map(|service| {
            service.mounts.iter().filter_map(move |mount| match mount {
                Mount::Named { name, location, .. } if !is_declared(name) => Some(RULE.finding(
                    Some(&service.name),
                    *location,
                    format!("named volume '{name}' is not declared in the root 'volumes' section"),
                    format!(
                        "Declare '{name}' under the root 'volumes' key (add 'external: true' if \
                         it is created outside this project), or fix the volume name."
                    ),
                )),
                Mount::Malformed {
                    raw,
                    reason,
                    location,
                } => Some(RULE.finding(
                    Some(&service.name),
                    *location,
                    format!("volume entry '{raw}' is malformed: {reason}"),
                    "Write the entry as 'source:target[:mode]' with mode 'ro' or 'rw', or use \
                     the long syntax (type, source, target).",
                )),
                _ => None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Declared, HostPathKind, Service};
    use crate::test_support::{at, declared, host_path, project, service};

    fn named(name: &str, line: u32) -> Mount {
        Mount::Named {
            name: name.into(),
            target: "/data".into(),
            location: at(line),
        }
    }

    fn audit(mounts: Vec<Mount>, volumes: Vec<Declared>) -> Vec<Finding> {
        check(&Project {
            volumes,
            ..project(vec![Service {
                mounts,
                ..service("db", 1)
            }])
        })
    }

    #[test]
    fn reports_an_undeclared_named_volume_at_its_location() {
        let findings = audit(vec![named("pgdata", 4)], Vec::new());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location.line, 4);
        assert_eq!(findings[0].service.as_deref(), Some("db"));
        assert!(findings[0].message.contains("'pgdata'"));
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn reports_a_malformed_entry_with_its_reason() {
        let malformed = Mount::Malformed {
            raw: "data:/data:rx".into(),
            reason: "unknown mode 'rx'".into(),
            location: at(5),
        };
        let findings = audit(vec![malformed], Vec::new());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location.line, 5);
        assert!(findings[0].message.contains("unknown mode 'rx'"));
        assert!(findings[0].message.contains("data:/data:rx"));
    }

    #[test]
    fn declared_volumes_are_fine() {
        assert!(audit(vec![named("pgdata", 4)], vec![declared("pgdata", 10)]).is_empty());
    }

    #[test]
    fn external_volumes_count_as_declared() {
        let external = Declared {
            external: true,
            ..declared("pgdata", 10)
        };
        assert!(audit(vec![named("pgdata", 4)], vec![external]).is_empty());
    }

    #[test]
    fn bind_and_anonymous_mounts_need_no_declaration() {
        let mounts = vec![
            Mount::Bind {
                host_path: host_path("./conf", HostPathKind::Bind, true, 4),
                target: "/conf".into(),
            },
            Mount::Anonymous {
                target: "/tmp".into(),
                location: at(5),
            },
        ];
        assert!(audit(mounts, Vec::new()).is_empty());
    }

    #[test]
    fn reports_each_offending_mount() {
        let findings = audit(
            vec![named("a", 4), named("b", 5), named("c", 6)],
            vec![declared("b", 10)],
        );
        let lines: Vec<u32> = findings.iter().map(|f| f.location.line).collect();
        assert_eq!(lines, [4, 6]);
    }
}
