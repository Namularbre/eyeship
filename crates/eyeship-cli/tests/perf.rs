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
