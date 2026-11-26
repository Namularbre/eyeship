//! `eyeship ui` end to end (spec 004 T020, T024): a real server on a free
//! port, queried with plain HTTP over a `TcpStream`.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const RULE_IDS: [&str; 17] = [
    "R001", "R002", "R003", "R004", "R005", "R006", "R007", "R008", "R009", "R010", "R011", "R012",
    "R013", "R014", "R015", "R016", "R017",
];

/// A running `eyeship ui`, killed when dropped.
struct Ui {
    child: Child,
    address: String,
}

impl Drop for Ui {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start(dir: &Path) -> Ui {
    let mut child = Command::new(env!("CARGO_BIN_EXE_eyeship"))
        .args(["ui", "--port", "0"])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("eyeship ui starts");
    let mut line = String::new();
    BufReader::new(child.stderr.take().expect("stderr is piped"))
        .read_line(&mut line)
        .expect("the address is printed");
    let address = line
        .split("http://")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("unexpected first line: {line}"))
        .to_owned();
    Ui { child, address }
}

/// Status code and body of `GET path`.
fn get(ui: &Ui, path: &str, host: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(&ui.address).expect("the server accepts connections");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("timeout is set");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is sent");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .unwrap_or_default();
    (status, body)
}

fn local(ui: &Ui, path: &str) -> (u16, String) {
    get(ui, path, &ui.address.replace("127.0.0.1", "localhost"))
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/compose")
        .canonicalize()
        .expect("the fixtures exist")
}

const CLEAN: &str = "services:\n  web:\n    image: nginx:1.27\n    restart: unless-stopped\n    mem_limit: 256m\n    logging:\n      driver: json-file\n      options:\n        max-size: 10m\n";
const CONFLICT: &str = "services:\n  a:\n    image: nginx:1.27\n    ports: [\"80:80\"]\n  b:\n    image: nginx:1.27\n    ports: [\"80:8080\"]\n";

#[test]
fn serves_the_dashboard_reports_history_and_rules() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fs::write(
        dir.path().join(".eyeship.toml"),
        "[deployments.prod]\nfiles = [\"compose.yaml\"]\n[deployments.dev]\nfiles = [\"missing.yaml\"]\n",
    )
    .expect("config is written");
    fs::write(dir.path().join("compose.yaml"), CONFLICT).expect("compose is written");
    let saved = Command::new(env!("CARGO_BIN_EXE_eyeship"))
        .args([
            "compose",
            "--deployment",
            "prod",
            "--save",
            "--label",
            "first",
        ])
        .current_dir(dir.path())
        .output()
        .expect("eyeship runs");
    assert_eq!(saved.status.code(), Some(1));

    let ui = start(dir.path());
    let (status, home) = local(&ui, "/");
    assert_eq!(status, 200);
    assert!(home.contains("/deployments/prod") && home.contains("/deployments/dev"));
    assert!(home.contains("0 of 2 deployments ready"), "{home}");

    let (status, prod) = local(&ui, "/deployments/prod");
    assert_eq!(status, 200);
    assert!(prod.contains("R009") && prod.contains("<code>first</code>"));
    let id = prod
        .split("/deployments/prod/reports/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("a saved report is listed")
        .to_owned();
    let (status, report) = local(&ui, &format!("/deployments/prod/reports/{id}"));
    assert_eq!(status, 200);
    assert!(report.contains("Saved report from"));

    // A reload shows the files as they are now (US2 §4).
    fs::write(dir.path().join("compose.yaml"), CLEAN).expect("compose is rewritten");
    let (_, prod) = local(&ui, "/deployments/prod");
    assert!(prod.contains("No issues found."), "{prod}");
    let (status, compare) = local(&ui, &format!("/deployments/prod/compare?from={id}&to=live"));
    assert_eq!(status, 200);
    assert!(compare.contains("<h2>New (0)</h2>"), "{compare}");
    assert!(compare.contains("<h2>Unchanged (0)</h2>"), "{compare}");
    assert!(!compare.contains("<h2>Resolved (0)</h2>"), "{compare}");

    assert_eq!(local(&ui, "/rules/R001").0, 200);
    assert_eq!(local(&ui, "/health"), (200, "ok\n".to_owned()));
    assert_eq!(local(&ui, "/nope").0, 404);
    assert_eq!(get(&ui, "/", "evil.example").0, 421);
}

#[test]
fn the_web_report_lists_exactly_the_findings_of_the_json_output() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let config: String = RULE_IDS
        .iter()
        .map(|rule| {
            let file = fixtures().join(rule).join("bad.yaml");
            format!(
                "[deployments.{}]\nfiles = [{:?}]\n",
                rule.to_lowercase(),
                file.display().to_string()
            )
        })
        .collect();
    fs::write(dir.path().join(".eyeship.toml"), config).expect("config is written");
    let ui = start(dir.path());
    for rule in RULE_IDS {
        let json = Command::new(env!("CARGO_BIN_EXE_eyeship"))
            .args([
                "compose",
                "--deployment",
                &rule.to_lowercase(),
                "--format",
                "json",
            ])
            .current_dir(dir.path())
            .output()
            .expect("eyeship runs");
        let value: serde_json::Value =
            serde_json::from_slice(&json.stdout).expect("stdout is JSON");
        let mut expected: Vec<String> = value["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|finding| finding["rule"].as_str().map(String::from))
            .collect();
        let (status, page) = local(&ui, &format!("/deployments/{}", rule.to_lowercase()));
        assert_eq!(status, 200, "{rule}");
        let mut shown: Vec<String> = page
            .split("<li class=\"finding\">")
            .skip(1)
            .filter_map(|item| {
                item.split("<a href=\"/rules/")
                    .nth(1)
                    .and_then(|rest| rest.split('"').next())
                    .map(String::from)
            })
            .collect();
        // The page groups by service; the multiset of findings is the same.
        expected.sort();
        shown.sort();
        assert_eq!(shown, expected, "{rule}");
    }
}

#[test]
fn a_port_in_use_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let ui = start(dir.path());
    let port = ui.address.rsplit(':').next().expect("a port").to_owned();
    let output = Command::new(env!("CARGO_BIN_EXE_eyeship"))
        .args(["ui", "--port", &port])
        .current_dir(dir.path())
        .output()
        .expect("eyeship runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot listen"));
}
