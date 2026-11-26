//! ENF1 benchmark: auditing a 50-service Compose file takes less than 100 ms.
//!
//! Ignored by default because timings are only meaningful on an optimised
//! build. Run it with:
//!
//! ```text
//! cargo test --release -p eyeship-cli --test perf -- --ignored
//! ```
//!
//! Memory (< 20 MB) is measured by `scripts/bench.sh`.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

const SERVICES: usize = 50;
const RUNS: usize = 5;
const BUDGET: Duration = Duration::from_millis(100);

/// A realistic stack: every service has dependencies, a healthcheck, ports,
/// volumes, networks, environment, logging and resource limits.
fn compose_with(services: usize) -> String {
    let mut yaml = String::from("name: bench\nservices:\n");
    (0..services).for_each(|i| {
        let depends_on = if i == 0 {
            String::new()
        } else {
            format!(
                "    depends_on:\n      svc-{:02}:\n        condition: service_healthy\n",
                i - 1
            )
        };
        let _ = write!(
            yaml,
            "  svc-{i:02}:\n    image: registry.example.com/team/svc-{i:02}:1.{i}.0\n    \
             restart: unless-stopped\n{depends_on}    environment:\n      \
             LOG_LEVEL: ${{LOG_LEVEL:-info}}\n      SERVICE_NAME: svc-{i:02}\n    \
             ports:\n      - \"{port}:8080\"\n    volumes:\n      - data-{i:02}:/var/lib/app\n    \
             networks: [backend]\n    healthcheck:\n      \
             test: [\"CMD\", \"curl\", \"-f\", \"http://localhost:8080/health\"]\n      \
             interval: 10s\n      timeout: 3s\n      retries: 5\n    \
             logging:\n      driver: json-file\n      options:\n        max-size: 10m\n        max-file: \"3\"\n    \
             deploy:\n      resources:\n        limits:\n          memory: 256M\n",
            port = 10_000 + i,
        );
    });
    yaml.push_str("networks:\n  backend: {}\nvolumes:\n");
    (0..services).for_each(|i| {
        let _ = writeln!(yaml, "  data-{i:02}: {{}}");
    });
    yaml
}

#[test]
#[ignore = "benchmark: run with --release -- --ignored"]
fn perf_audits_fifty_services_under_100_ms() {
    let dir: PathBuf = std::env::temp_dir().join(format!("eyeship-perf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir is created");
    let file = dir.join("compose.yaml");
    std::fs::write(&file, compose_with(SERVICES)).expect("compose file is written");

    let run = || {
        let start = Instant::now();
        let output = Command::new(env!("CARGO_BIN_EXE_eyeship"))
            .arg("compose")
            .arg(&file)
            .current_dir(&dir)
            .output()
            .expect("eyeship runs");
        (start.elapsed(), output.status.code())
    };

    let (_, code) = run(); // warm-up (page cache)
    let best = (0..RUNS).map(|_| run().0).min().expect("at least one run");
    let _ = std::fs::remove_dir_all(&dir);

    assert!(matches!(code, Some(0 | 1)), "unexpected exit code {code:?}");
    assert!(
        best < BUDGET,
        "50 services audited in {best:?}, budget {BUDGET:?}"
    );
    eprintln!("50 services audited in {best:?} (best of {RUNS})");
}

/// Spec 004 ENF5 and ENF8: the report page of 50 services renders under
/// 100 ms, and a history of 1,000 saved reports under 300 ms.
#[test]
#[ignore = "benchmark: run with --release -- --ignored"]
fn perf_web_pages_of_fifty_services_and_a_thousand_saved_reports() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::process::Stdio;

    let dir = tempfile::tempdir().expect("a temporary directory");
    // Without `restart`, each service has a finding: the page is not empty.
    let compose = compose_with(SERVICES).replace("    restart: unless-stopped\n", "");
    std::fs::write(dir.path().join("compose.yaml"), compose).expect("compose file is written");
    std::fs::write(
        dir.path().join(".eyeship.toml"),
        "[deployments.live]\nfiles = [\"compose.yaml\"]\n[deployments.history]\nfiles = [\"compose.yaml\"]\n",
    )
    .expect("config is written");
    let eyeship = || Command::new(env!("CARGO_BIN_EXE_eyeship"));
    eyeship()
        .args(["compose", "--deployment", "history", "--save"])
        .current_dir(dir.path())
        .output()
        .expect("eyeship runs");
    let history = dir.path().join(".eyeship/reports/history");
    let saved = std::fs::read_dir(&history)
        .expect("the history exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .next()
        .expect("one saved report");
    let content = std::fs::read_to_string(&saved).expect("readable");
    (1..1000).for_each(|i| {
        let id = format!("2026-01-01T00-00-00.{i:03}Z");
        std::fs::write(
            history.join(format!("{id}.sarif")),
            content.replace(
                saved
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default(),
                &id,
            ),
        )
        .expect("a copy is written");
    });

    let mut child = eyeship()
        .args(["ui", "--port", "0"])
        .current_dir(dir.path())
        .stderr(Stdio::piped())
        .spawn()
        .expect("eyeship ui starts");
    let mut line = String::new();
    BufReader::new(child.stderr.take().expect("stderr"))
        .read_line(&mut line)
        .expect("the address is printed");
    let address = line
        .split("http://")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("an address")
        .to_owned();
    let time = |path: &str| {
        (0..RUNS)
            .map(|_| {
                let start = Instant::now();
                let mut stream = TcpStream::connect(&address).expect("connects");
                write!(
                    stream,
                    "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                )
                .expect("sent");
                let mut response = String::new();
                stream.read_to_string(&mut response).expect("read");
                assert!(response.starts_with("HTTP/1.1 200"), "{path}");
                start.elapsed()
            })
            .min()
            .expect("at least one run")
    };
    let live = time("/deployments/live");
    let history = time("/deployments/history");
    let dashboard = time("/");
    let _ = child.kill();
    let _ = child.wait();
    eprintln!("live report {live:?}, history of 1000 {history:?}, dashboard {dashboard:?}");
    assert!(live < BUDGET, "live report page in {live:?}");
    assert!(
        history < Duration::from_millis(300),
        "history page in {history:?}"
    );
    assert!(
        dashboard < Duration::from_millis(300),
        "dashboard in {dashboard:?}"
    );
}
