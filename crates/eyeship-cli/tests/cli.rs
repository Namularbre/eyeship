//! End-to-end tests of the `eyeship` binary.

use assert_cmd::Command;

fn eyeship() -> Command {
    Command::cargo_bin("eyeship").expect("the eyeship binary is built")
}

#[test]
fn rules_lists_the_catalogue() {
    let output = eyeship().arg("rules").output().expect("eyeship runs");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("ID"));
}

#[test]
fn explain_an_unknown_rule_fails_with_a_usage_error() {
    let output = eyeship()
        .args(["explain", "R999"])
        .output()
        .expect("eyeship runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown rule 'R999'"));
}

#[test]
fn help_succeeds() {
    let output = eyeship().arg("--help").output().expect("eyeship runs");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("compose"));
}

#[test]
fn an_invalid_threshold_is_a_usage_error() {
    let output = eyeship()
        .args(["compose", "--fail-on", "loud"])
        .output()
        .expect("eyeship runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("loud"));
}

#[test]
fn a_missing_subcommand_is_a_usage_error() {
    let output = eyeship().output().expect("eyeship runs");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn compose_is_not_wired_yet() {
    let output = eyeship()
        .args(["compose", "compose.yaml"])
        .output()
        .expect("eyeship runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not implemented yet"));
}
