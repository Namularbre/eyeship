//! SARIF output (spec 004 US10): valid against the official 2.1.0 schema for
//! every fixture, deterministic, and consistent with the JSON output.

use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;

const RULE_IDS: [&str; 17] = [
    "R001", "R002", "R003", "R004", "R005", "R006", "R007", "R008", "R009", "R010", "R011", "R012",
    "R013", "R014", "R015", "R016", "R017",
];

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn audit(rule: &str, file: &str, format: &str) -> Output {
    Command::cargo_bin("eyeship")
        .expect("the eyeship binary is built")
        .current_dir(fixtures().join("compose").join(rule))
        .args(["compose", file, "--format", format])
        .output()
        .expect("eyeship runs")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is pure JSON")
}

fn validator() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("sarif/sarif-2.1.0.schema.json"))
            .expect("the schema is readable"),
    )
    .expect("the schema is JSON");
    jsonschema::validator_for(&schema).expect("the schema compiles")
}

#[test]
fn every_fixture_produces_valid_sarif() {
    let validator = validator();
    let invalid: Vec<String> = RULE_IDS
        .iter()
        .flat_map(|rule| ["bad.yaml", "good.yaml"].map(|file| (rule, file)))
        .filter_map(|(rule, file)| {
            let document = json(&audit(rule, file, "sarif"));
            let errors: Vec<String> = validator
                .iter_errors(&document)
                .map(|error| format!("{rule}/{file}: {error} at {}", error.instance_path()))
                .collect();
            (!errors.is_empty()).then(|| errors.join("\n"))
        })
        .collect();
    assert!(invalid.is_empty(), "{}", invalid.join("\n"));
}

#[test]
fn sarif_has_the_same_findings_and_exit_code_as_json() {
    for rule in RULE_IDS {
        let (sarif, json_output) = (
            audit(rule, "bad.yaml", "sarif"),
            audit(rule, "bad.yaml", "json"),
        );
        assert_eq!(sarif.status.code(), json_output.status.code(), "{rule}");
        let rules = |values: &Value, key: &str| -> Vec<String> {
            values
                .as_array()
                .into_iter()
                .flatten()
                .filter(|result| result.get("suppressions").is_none())
                .filter_map(|result| result[key].as_str().map(String::from))
                .collect()
        };
        assert_eq!(
            rules(&json(&sarif)["runs"][0]["results"], "ruleId"),
            rules(&json(&json_output)["findings"], "rule"),
            "{rule}"
        );
    }
}

#[test]
fn sarif_output_is_deterministic() {
    let first = audit("R009", "bad.yaml", "sarif");
    let second = audit("R009", "bad.yaml", "sarif");
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
}

#[test]
fn the_schema_rejects_an_invalid_document() {
    let mut document = json(&audit("R009", "bad.yaml", "sarif"));
    document["runs"][0]["results"][0]["level"] = Value::from("fatal");
    assert!(!validator().is_valid(&document));
}
