//! Declared deployments (spec 004 US1, US5): `eyeship compose --deployment`.

use std::fs;
use std::process::Output;

use assert_cmd::Command;

const CONFIG: &str = "[deployments.staging]\nfiles = [\"compose.yaml\"]\n\
    [deployments.prod]\nfiles = [\"compose.yaml\", \"compose.prod.yaml\"]\nenv_file = \".env.prod\"\n";
const BASE: &str = "services:\n  api:\n    image: nginx:1.27\n    restart: unless-stopped\n    mem_limit: 256m\n    logging:\n      driver: json-file\n      options:\n        max-size: 10m\n    environment:\n      TOKEN: ${TOKEN}\n";
const PROD: &str = "services:\n  api:\n    image: nginx:latest\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in [
        (".eyeship.toml", CONFIG),
        ("compose.yaml", BASE),
        ("compose.prod.yaml", PROD),
        (".env", "TOKEN=dev\n"),
        (".env.prod", "TOKEN=prod\n"),
    ] {
        fs::write(dir.path().join(name), content).expect("file is written");
    }
    dir
}

fn eyeship(dir: &tempfile::TempDir, args: &[&str]) -> Output {
    Command::cargo_bin("eyeship")
        .expect("the eyeship binary is built")
        .current_dir(dir.path())
        .env_remove("TOKEN")
        .args(args)
        .output()
        .expect("eyeship runs")
}

fn rules(output: &Output) -> Vec<String> {
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the output is JSON");
    report["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|finding| finding["rule"].as_str().map(String::from))
        .collect()
}

#[test]
fn a_deployment_merges_its_files_and_uses_its_env_file() {
    let dir = project();
    let output = eyeship(
        &dir,
        &["compose", "--deployment", "prod", "--format", "json"],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(rules(&output), ["R008"]);
}

#[test]
fn a_deployment_gives_the_same_result_as_the_equivalent_command() {
    let dir = project();
    let by_name = eyeship(
        &dir,
        &["compose", "--deployment", "prod", "--format", "json"],
    );
    let explicit = eyeship(
        &dir,
        &[
            "compose",
            "-f",
            "compose.yaml",
            "-f",
            "compose.prod.yaml",
            "--env-file",
            ".env.prod",
            "--format",
            "json",
        ],
    );
    assert_eq!(by_name.stdout, explicit.stdout);
    assert_eq!(by_name.status.code(), explicit.status.code());
}

#[test]
fn an_unknown_deployment_is_a_usage_error() {
    let output = eyeship(&project(), &["compose", "--deployment", "dev"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("unknown deployment 'dev' (declared: prod, staging)")
    );
}

#[test]
fn a_deployment_with_explicit_files_is_a_usage_error() {
    let output = eyeship(
        &project(),
        &["compose", "compose.yaml", "--deployment", "prod"],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
