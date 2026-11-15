//! End-to-end tests of `.eyeship.toml` (T042) and `--env-file` (T044).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;

/// Two services publishing the same host port: R009 (error).
const CONFLICT: &str = "services:\n  a:\n    image: nginx:1.27\n    ports: [\"80:80\"]\n  b:\n    image: nginx:1.27\n    ports: [\"80:8080\"]\n";

fn eyeship_in(dir: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("eyeship")
        .expect("the eyeship binary is built")
        .current_dir(dir)
        .env_remove("EYESHIP_TEST_TAG")
        .args(args)
        .output()
        .expect("eyeship runs")
}

fn project(config: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fs::write(dir.path().join("compose.yaml"), CONFLICT).expect("compose.yaml is written");
    if let Some(config) = config {
        fs::write(dir.path().join(".eyeship.toml"), config).expect(".eyeship.toml is written");
    }
    dir
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn without_a_config_file_the_conflict_fails() {
    let dir = project(None);
    assert_eq!(eyeship_in(dir.path(), &["compose"]).status.code(), Some(1));
}

#[test]
fn the_config_file_ignores_rules_and_counts_them() {
    let dir = project(Some("ignore = [\"R009\"]\n"));
    let output = eyeship_in(dir.path(), &["compose"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).contains("ignored by configuration"));
}

#[test]
fn the_config_threshold_yields_to_an_explicit_fail_on() {
    let dir = project(Some("fail_on = \"none\"\n"));
    assert_eq!(eyeship_in(dir.path(), &["compose"]).status.code(), Some(0));
    let output = eyeship_in(dir.path(), &["compose", "--fail-on", "error"]);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn an_unknown_rule_in_the_config_file_is_a_usage_error() {
    let dir = project(Some("[services.a]\nignore = [\"R99\"]\n"));
    let output = eyeship_in(dir.path(), &["compose"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(".eyeship.toml"), "{stderr}");
    assert!(stderr.contains("unknown rule 'R99'"), "{stderr}");
}

#[test]
fn invalid_toml_is_a_usage_error() {
    let dir = project(Some("ignore = ["));
    let output = eyeship_in(dir.path(), &["compose"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains(".eyeship.toml"));
}

fn env_file_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/compose/env-file")
}

#[test]
fn an_undefined_variable_triggers_r013() {
    let output = eyeship_in(&env_file_fixture(), &["compose", "--fail-on", "none"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("R013"), "{}", stdout(&output));
}

#[test]
fn the_env_file_defines_the_variable() {
    let output = eyeship_in(
        &env_file_fixture(),
        &["compose", "--env-file", "prod.env", "--fail-on", "none"],
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(!stdout(&output).contains("R013"), "{}", stdout(&output));
}

#[test]
fn a_missing_env_file_is_a_usage_error() {
    let output = eyeship_in(
        &env_file_fixture(),
        &["compose", "--env-file", "missing.env"],
    );
    assert_eq!(output.status.code(), Some(2));
}
