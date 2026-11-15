//! End-to-end tests: each rule's `bad.yaml` triggers it, its `good.yaml`
//! does not, and the full output is compared to snapshots.

use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;

const RULE_IDS: [&str; 17] = [
    "R001", "R002", "R003", "R004", "R005", "R006", "R007", "R008", "R009", "R010", "R011", "R012",
    "R013", "R014", "R015", "R016", "R017",
];

fn fixture_dir(rule: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/compose")
        .join(rule)
}

fn audit(rule: &str, file: &str, extra: &[&str]) -> Output {
    Command::cargo_bin("eyeship")
        .expect("the eyeship binary is built")
        .current_dir(fixture_dir(rule))
        .env_remove("NO_COLOR")
        .args(["compose", file])
        .args(extra)
        .output()
        .expect("eyeship runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn rule_ids(output: &Output) -> Vec<String> {
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is pure JSON");
    json["findings"]
        .as_array()
        .map(|findings| {
            findings
                .iter()
                .filter_map(|finding| finding["rule"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn every_bad_fixture_triggers_its_rule() {
    let missing: Vec<&str> = RULE_IDS
        .into_iter()
        .filter(|rule| {
            let ids = rule_ids(&audit(rule, "bad.yaml", &["--format", "json"]));
            !ids.iter().any(|id| id == rule)
        })
        .collect();
    assert!(missing.is_empty(), "not triggered: {missing:?}");
}

#[test]
fn no_good_fixture_triggers_its_rule() {
    let triggered: Vec<&str> = RULE_IDS
        .into_iter()
        .filter(|rule| {
            let ids = rule_ids(&audit(rule, "good.yaml", &["--format", "json"]));
            ids.iter().any(|id| id == rule)
        })
        .collect();
    assert!(triggered.is_empty(), "false positives: {triggered:?}");
}

#[test]
fn text_output_of_bad_fixtures() {
    for rule in RULE_IDS {
        let output = audit(rule, "bad.yaml", &["--fail-on", "none"]);
        assert_eq!(output.status.code(), Some(0), "{rule}");
        insta::assert_snapshot!(format!("text_{rule}"), stdout(&output));
    }
}

#[test]
fn json_output_of_bad_fixtures() {
    for rule in RULE_IDS {
        let output = audit(rule, "bad.yaml", &["--format", "json", "--fail-on", "none"]);
        insta::assert_snapshot!(format!("json_{rule}"), stdout(&output));
    }
}

#[test]
fn r008_end_to_end_fails_on_warning_only_when_asked() {
    assert_eq!(audit("R008", "bad.yaml", &[]).status.code(), Some(0));
    assert_eq!(
        audit("R008", "bad.yaml", &["--fail-on", "warning"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        audit("R008", "good.yaml", &["--fail-on", "warning"])
            .status
            .code(),
        Some(0)
    );
}

#[test]
fn fail_on_thresholds() {
    // R017 bad fixture: only info findings.
    let code = |threshold: &str| {
        audit("R017", "bad.yaml", &["--fail-on", threshold])
            .status
            .code()
    };
    assert_eq!(code("error"), Some(0));
    assert_eq!(code("warning"), Some(0));
    assert_eq!(code("info"), Some(1));
    assert_eq!(code("none"), Some(0));
}

#[test]
fn an_invalid_yaml_file_exits_with_2() {
    let output = audit("R001", "../../../../Cargo.toml", &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn the_default_file_is_discovered() {
    let output = Command::cargo_bin("eyeship")
        .expect("the eyeship binary is built")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["compose"])
        .output()
        .expect("eyeship runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("compose"));
}
