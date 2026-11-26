use super::*;
use eyeship_domain::{FailOn, FileId, Finding, Location, Report, RuleId, Severity};
use eyeship_history::{MemoryStore, SaveRequest, list, save};
use std::path::Path;

const ROOT: &str = "/reports";

fn finding(rule: &'static str, severity: Severity, service: &str, message: &str) -> Finding {
    Finding {
        rule: RuleId(rule),
        severity,
        service: Some(service.into()),
        location: Location::new(FileId(0), 3, 5),
        message: message.into(),
        fix: "fix it".into(),
    }
}

fn report(findings: Vec<Finding>) -> Report {
    Report {
        files: vec!["compose.yaml".into()],
        findings,
        ignored: Vec::new(),
    }
}

struct Fake {
    deployments: Result<Vec<DeploymentResult>, String>,
    store: MemoryStore,
}

impl DeploymentSource for Fake {
    fn deployments(&self) -> Result<Vec<DeploymentResult>, String> {
        self.deployments.clone()
    }
    fn history_names(&self) -> Vec<String> {
        eyeship_history::deployments(&self.store, Path::new(ROOT))
    }
    fn history(&self, name: &str) -> Vec<Entry> {
        list(&self.store, Path::new(ROOT), name)
    }
}

fn fake() -> Fake {
    let store = MemoryStore::default();
    let older = report(vec![
        finding("R009", Severity::Error, "web", "port 80 is published twice"),
        finding("R006", Severity::Warning, "api", "no memory limit"),
    ]);
    for (deployment, report, now) in [
        ("prod", &older, 1_764_000_000_000),
        ("prod", &report(Vec::new()), 1_764_100_000_000),
        ("old", &older, 1_760_000_000_000),
    ] {
        save(
            &store,
            Path::new(ROOT),
            report,
            &SaveRequest {
                deployment,
                fail_on: FailOn::Error,
                label: Some("a1b2c3d"),
                now_millis: now,
                tool_version: "0.1.0",
            },
        )
        .expect("saved");
    }
    Fake {
        deployments: Ok(vec![
            DeploymentResult {
                name: "broken".into(),
                files: vec!["missing.yaml".into()],
                fail_on: FailOn::Error,
                outcome: Err("file not found: missing.yaml".into()),
            },
            DeploymentResult {
                name: "prod".into(),
                files: vec!["compose.yaml".into()],
                fail_on: FailOn::Error,
                outcome: Ok(report(vec![
                    finding("R006", Severity::Warning, "api", "no memory limit"),
                    finding(
                        "R001",
                        Severity::Warning,
                        "api",
                        "depends_on 'db' is not healthy",
                    ),
                ])),
            },
        ]),
        store,
    }
}

fn get(source: &Fake, url: &str) -> Response {
    handle(
        &Request {
            method: "GET",
            url,
            host: Some("127.0.0.1:7878"),
        },
        source,
        true,
    )
}

fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

#[test]
fn the_dashboard_shows_declared_and_history_only_deployments() {
    let response = get(&fake(), "/");
    assert_eq!(response.status, 200);
    let body = &response.body;
    let (broken, prod, old) = (
        body.find("/deployments/broken").expect("broken"),
        body.find("/deployments/prod").expect("prod"),
        body.find("/deployments/old").expect("old"),
    );
    assert!(broken < prod && prod < old);
    assert!(body.contains("history only"));
    assert!(
        body.contains("▲ 2 more since the last saved report"),
        "{body}"
    );
    assert!(body.contains("1 of 3 deployments ready"));
}

#[test]
fn a_deployment_page_shows_the_live_report_and_its_history() {
    let response = get(&fake(), "/deployments/prod?severity=warning");
    assert_eq!(response.status, 200);
    assert!(response.body.contains("Live report"));
    assert!(
        response
            .body
            .contains("<option value=\"warning\" selected>")
    );
    assert_eq!(
        response.body.matches("/deployments/prod/reports/").count(),
        2
    );
    assert!(response.body.contains("<svg class=\"chart\""));
    assert!(
        response
            .body
            .contains("action=\"/deployments/prod/compare\"")
    );
}

#[test]
fn a_history_only_deployment_has_a_page() {
    let response = get(&fake(), "/deployments/old");
    assert_eq!(response.status, 200);
    assert!(response.body.contains("not declared"));
}

#[test]
fn saved_reports_and_comparisons() {
    let source = fake();
    let ids: Vec<String> = source.history("prod").into_iter().map(|e| e.id).collect();
    let saved = get(&source, &format!("/deployments/prod/reports/{}", ids[1]));
    assert_eq!(saved.status, 200);
    assert!(saved.body.contains("Saved report from"));
    assert!(saved.body.contains("port 80 is published twice"));
    let compare = get(
        &source,
        &format!("/deployments/prod/compare?from={}&to=live", ids[1]),
    );
    assert_eq!(compare.status, 200, "{}", compare.body);
    assert!(compare.body.contains("<h2>New (1)</h2>"));
    assert!(compare.body.contains("<h2>Resolved (1)</h2>"));
    assert!(compare.body.contains("<h2>Unchanged (1)</h2>"));
    assert_eq!(
        get(&source, "/deployments/prod/compare?from=nope&to=live").status,
        404
    );
}

#[test]
fn unknown_things_are_not_found() {
    let source = fake();
    for url in [
        "/deployments/dev",
        "/deployments/prod/reports/nope",
        "/deployments/../reports/x",
        "/deployments/%2e%2e/reports/x",
        "/rules/R999",
        "/assets/style.00000000.css",
        "/nope",
    ] {
        assert_eq!(get(&source, url).status, 404, "{url}");
    }
}

#[test]
fn serves_rules_style_and_health() {
    let source = fake();
    assert!(get(&source, "/rules").body.contains("/rules/R017"));
    assert!(
        get(&source, "/rules/r001")
            .body
            .contains("Failure scenario")
    );
    let style = get(&source, &style_path());
    assert_eq!(style.status, 200);
    assert_eq!(
        header(&style, "Content-Type"),
        Some("text/css; charset=utf-8")
    );
    assert!(header(&style, "Cache-Control").is_some_and(|v| v.contains("immutable")));
    assert_eq!(get(&source, "/health").body, "ok\n");
}

#[test]
fn every_response_carries_the_security_headers() {
    let source = fake();
    for url in ["/", "/deployments/prod", "/nope", "/health"] {
        let response = get(&source, url);
        assert_eq!(
            header(&response, "Content-Security-Policy"),
            Some(CSP),
            "{url}"
        );
        assert_eq!(header(&response, "X-Content-Type-Options"), Some("nosniff"));
        assert_eq!(header(&response, "Referrer-Policy"), Some("no-referrer"));
    }
    assert_eq!(
        header(&get(&source, "/"), "Cache-Control"),
        Some("no-store")
    );
}

#[test]
fn only_reads_are_allowed() {
    let response = handle(
        &Request {
            method: "POST",
            url: "/",
            host: Some("localhost"),
        },
        &fake(),
        true,
    );
    assert_eq!(response.status, 405);
    assert_eq!(header(&response, "Allow"), Some("GET, HEAD"));
}

#[test]
fn a_local_server_rejects_foreign_hosts() {
    let request = |host| Request {
        method: "GET",
        url: "/",
        host,
    };
    let source = fake();
    for host in [
        Some("evil.example"),
        Some("127.0.0.1.evil.example:7878"),
        None,
    ] {
        assert_eq!(
            handle(&request(host), &source, true).status,
            421,
            "{host:?}"
        );
    }
    for host in [
        "localhost",
        "localhost:7878",
        "127.0.0.1:1",
        "[::1]:7878",
        "LOCALHOST",
    ] {
        assert_eq!(
            handle(&request(Some(host)), &source, true).status,
            200,
            "{host}"
        );
    }
    assert_eq!(
        handle(&request(Some("my-vps.example")), &source, false).status,
        200
    );
}

#[test]
fn an_invalid_configuration_is_reported() {
    let source = Fake {
        deployments: Err("invalid configuration file .eyeship.toml: oops".into()),
        store: MemoryStore::default(),
    };
    let response = get(&source, "/");
    assert_eq!(response.status, 500);
    assert!(response.body.contains("oops"));
}
