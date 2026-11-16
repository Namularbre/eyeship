//! Real-world corpus (`tests/corpus/<project>/`): every public Compose file
//! loads (never exit code 2) and its JSON report is pinned by a snapshot, so
//! any new finding on real files is reviewed for false positives.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

/// `(project, directory, compose file name)` for every corpus entry, sorted.
fn corpus() -> Vec<(String, PathBuf, String)> {
    let mut entries: Vec<(String, PathBuf, String)> = fs::read_dir(corpus_dir())
        .expect("the corpus directory exists")
        .map(|entry| entry.expect("a readable corpus entry").path())
        .filter(|path| path.is_dir())
        .map(|dir| {
            let project = file_name(&dir);
            let file = compose_file(&dir);
            (project, dir, file)
        })
        .collect();
    entries.sort();
    entries
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .expect("a named path")
        .to_string_lossy()
        .into_owned()
}

/// The single `*.yml` / `*.yaml` file of a corpus directory.
fn compose_file(dir: &Path) -> String {
    let files: Vec<String> = fs::read_dir(dir)
        .expect("a readable corpus directory")
        .map(|entry| file_name(&entry.expect("a readable corpus file").path()))
        .filter(|name| name.ends_with(".yml") || name.ends_with(".yaml"))
        .collect();
    assert_eq!(files.len(), 1, "one Compose file in {}", dir.display());
    files.into_iter().next().expect("one Compose file")
}

#[test]
fn the_corpus_has_at_least_ten_projects() {
    assert!(corpus().len() >= 10);
}

#[test]
fn every_corpus_file_loads_and_its_json_report_is_stable() {
    for (project, dir, file) in corpus() {
        // A clean environment: only the project's own `.env` (if committed)
        // provides variables, whatever the machine running the tests.
        let output = Command::cargo_bin("eyeship")
            .expect("the eyeship binary is built")
            .current_dir(&dir)
            .env_clear()
            .args(["compose", &file, "--fail-on", "none", "--format", "json"])
            .output()
            .expect("eyeship runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{project}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("stdout is pure JSON");
        insta::assert_snapshot!(
            format!("corpus_{project}"),
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
