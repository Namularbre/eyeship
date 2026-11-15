//! Hand-made reports shared by the renderer tests.

use eyeship_domain::{FileId, Finding, Location, Report, RuleId, Severity};

fn finding(
    rule: &'static str,
    severity: Severity,
    service: Option<&str>,
    location: (u32, u32, u32),
    message: &str,
    fix: &str,
) -> Finding {
    let (file, line, column) = location;
    Finding {
        rule: RuleId(rule),
        severity,
        service: service.map(str::to_owned),
        location: Location::new(FileId(file), line, column),
        message: message.to_owned(),
        fix: fix.to_owned(),
    }
}

/// Three findings on two services plus one project-level finding, already
/// sorted as the engine does, and two ignored findings.
pub fn sample_report() -> Report {
    Report {
        files: vec!["compose.yaml".into(), "compose.override.yaml".into()],
        findings: vec![
            finding(
                "R005",
                Severity::Error,
                Some("api"),
                (0, 4, 5),
                "volume 'data' is not declared at the root",
                "Declare 'data' under the root `volumes:` key",
            ),
            finding(
                "R002",
                Severity::Error,
                Some("db"),
                (0, 9, 12),
                "image 'postgres' has no tag",
                "Pin a version, e.g. `postgres:16.4`",
            ),
            finding(
                "R001",
                Severity::Warning,
                Some("api"),
                (1, 3, 7),
                "depends_on 'db' does not wait for the service to be healthy",
                "Use `condition: service_healthy` and add a healthcheck to 'db'",
            ),
            finding(
                "R017",
                Severity::Info,
                None,
                (0, 1, 1),
                "the root `version` key is obsolete",
                "Remove the `version` key",
            ),
        ],
        ignored: vec![
            finding("R004", Severity::Warning, Some("api"), (0, 2, 3), "m", "f"),
            finding("R004", Severity::Warning, Some("db"), (0, 8, 3), "m", "f"),
        ],
    }
}
