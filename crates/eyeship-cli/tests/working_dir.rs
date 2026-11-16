//! eyeship looks for the Compose file where it is run, like `docker compose`,
//! never at the root of the enclosing project.

use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;

const ROOT: &str = "services:\n  root:\n    image: nginx:1.27\n";
const SUB: &str = "services:\n  sub:\n    image: nginx:1.27\n";

fn eyeship_in(dir: &Path, args: &[&str]) -> Output {
    Command::cargo_bin("eyeship")
        .expect("the eyeship binary is built")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("eyeship runs")
}

/// A project with a Compose file at its root and another one in `sub/`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fs::create_dir(dir.path().join("sub")).expect("sub/ is created");
    fs::write(dir.path().join("compose.yaml"), ROOT).expect("root compose is written");
    fs::write(dir.path().join("sub/compose.yaml"), SUB).expect("sub compose is written");
    dir
}

/// The audited files and the services that have findings.
fn audited(output: &Output) -> (Vec<String>, Vec<String>) {
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the output is JSON");
    let strings = |values: Vec<&serde_json::Value>| -> Vec<String> {
        let mut all: Vec<String> = values
            .iter()
            .filter_map(|v| v.as_str())
            .map(String::from)
            .collect();
        all.dedup();
        all
    };
    let files = strings(report["files"].as_array().into_iter().flatten().collect());
    let services = strings(
        report["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| &f["service"])
            .collect(),
    );
    (files, services)
}

#[test]
fn run_from_a_subdirectory_audits_the_subdirectory_file() {
    let dir = project();
    let output = eyeship_in(&dir.path().join("sub"), &["compose", "--format", "json"]);
    assert_eq!(
        audited(&output),
        (vec!["compose.yaml".into()], vec!["sub".into()])
    );
}

#[test]
fn a_directory_argument_audits_the_file_inside_it() {
    let dir = project();
    let output = eyeship_in(dir.path(), &["compose", "sub", "--format", "json"]);
    assert_eq!(
        audited(&output),
        (vec!["sub/compose.yaml".into()], vec!["sub".into()])
    );
}
