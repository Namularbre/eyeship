//! R012: a host path used by a service does not exist.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{HostPathKind, Mount, Project};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R012"),
    title: "Missing host path",
    severity: Severity::Warning,
    doc: RuleDoc {
        scenario: "A bind mount, env_file, secret or config points to a host path that does not \
                   exist. Docker silently creates an empty directory for a bind mount, so the \
                   application starts without its configuration; a missing env_file, secret or \
                   config makes the deployment fail.",
        fix: "Create the file or directory before deploying, or fix the path (relative paths are \
              resolved from the Compose file's directory).",
        bad_example: "services:\n  web:\n    image: nginx:1.27.2\n    volumes:\n      - ./nginx.conf:/etc/nginx/nginx.conf:ro\n# ./nginx.conf is missing\n",
        good_example: "services:\n  web:\n    image: nginx:1.27.2\n    volumes:\n      - ./nginx.conf:/etc/nginx/nginx.conf:ro\n# ./nginx.conf is committed next to compose.yaml\n",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .flat_map(|service| {
            let binds = service.mounts.iter().filter_map(|mount| match mount {
                Mount::Bind { host_path, .. } => Some(host_path),
                _ => None,
            });
            binds
                .chain(&service.files)
                .filter(|path| !path.exists)
                .map(move |path| {
                    RULE.finding(
                        Some(&service.name),
                        path.location,
                        format!(
                            "{} '{}' does not exist on the host",
                            kind(path.kind),
                            path.path
                        ),
                        format!(
                            "Create '{}' before deploying, or fix the path (relative paths \
                             start from the Compose file's directory).",
                            path.path
                        ),
                    )
                })
        })
        .collect()
}

const fn kind(kind: HostPathKind) -> &'static str {
    match kind {
        HostPathKind::Bind => "bind mount source",
        HostPathKind::EnvFile => "env_file",
        HostPathKind::Secret => "secret file",
        HostPathKind::Config => "config file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{HostPath, Service};
    use crate::test_support::{at, host_path, project, service};

    fn bind(path: &str, exists: bool, line: u32) -> Mount {
        Mount::Bind {
            host_path: host_path(path, HostPathKind::Bind, exists, line),
            target: "/target".into(),
        }
    }

    fn audit(mounts: Vec<Mount>, files: Vec<HostPath>) -> Vec<Finding> {
        check(&project(vec![Service {
            mounts,
            files,
            ..service("web", 1)
        }]))
    }

    #[test]
    fn reports_a_missing_bind_mount_source() {
        let findings = audit(vec![bind("./nginx.conf", false, 4)], Vec::new());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].location.line, 4);
        assert_eq!(findings[0].service.as_deref(), Some("web"));
        assert!(findings[0].message.contains("bind mount"));
        assert!(findings[0].message.contains("'./nginx.conf'"));
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn reports_missing_env_files_secrets_and_configs_with_their_kind() {
        let files = vec![
            host_path("./.env.prod", HostPathKind::EnvFile, false, 5),
            host_path("./secrets/db.txt", HostPathKind::Secret, false, 20),
            host_path("./app.toml", HostPathKind::Config, false, 25),
        ];
        let findings = audit(Vec::new(), files);
        let messages: Vec<&str> = findings.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(messages.len(), 3);
        assert!(messages[0].contains("env_file") && messages[0].contains("'./.env.prod'"));
        assert!(messages[1].contains("secret") && messages[1].contains("'./secrets/db.txt'"));
        assert!(messages[2].contains("config") && messages[2].contains("'./app.toml'"));
    }

    #[test]
    fn existing_paths_are_fine() {
        let files = vec![host_path("./.env", HostPathKind::EnvFile, true, 5)];
        assert!(audit(vec![bind("./conf", true, 4)], files).is_empty());
    }

    #[test]
    fn named_anonymous_and_malformed_mounts_are_ignored() {
        let mounts = vec![
            Mount::Named {
                name: "data".into(),
                target: "/data".into(),
                location: at(4),
            },
            Mount::Anonymous {
                target: "/tmp".into(),
                location: at(5),
            },
            Mount::Malformed {
                raw: "::".into(),
                reason: "empty".into(),
                location: at(6),
            },
        ];
        assert!(audit(mounts, Vec::new()).is_empty());
    }

    #[test]
    fn only_missing_paths_are_reported() {
        let mounts = vec![bind("./a", true, 4), bind("./b", false, 5)];
        let files = vec![host_path("./c", HostPathKind::EnvFile, false, 6)];
        let lines: Vec<u32> = audit(mounts, files)
            .iter()
            .map(|f| f.location.line)
            .collect();
        assert_eq!(lines, [5, 6]);
    }
}
