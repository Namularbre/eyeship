//! Saving and reading the reports of each deployment.

use std::io;
use std::path::{Path, PathBuf};

use eyeship_domain::{FailOn, Report};
use eyeship_report::{SarifRecord, SavedContext, parse_sarif, render_sarif};

use crate::clock::{file_id, rfc3339};
use crate::store::Store;

const EXTENSION: &str = "sarif";

/// One saved report, or why it cannot be read (spec 004 US8 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub outcome: Result<SarifRecord, String>,
}

/// What to save, besides the report itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest<'a> {
    pub deployment: &'a str,
    pub fail_on: FailOn,
    pub label: Option<&'a str>,
    pub now_millis: u64,
    pub tool_version: &'a str,
}

/// Saves `report` as `<root>/<deployment>/<id>.sarif` and returns its path.
/// The ID is the date; a suffix (`-1`, `-2`…) avoids overwriting a report
/// saved in the same millisecond.
pub fn save(
    store: &dyn Store,
    root: &Path,
    report: &Report,
    request: &SaveRequest,
) -> io::Result<PathBuf> {
    let SaveRequest {
        deployment,
        fail_on,
        label,
        now_millis,
        tool_version,
    } = *request;
    let dir = root.join(deployment);
    let base = file_id(now_millis);
    let id = std::iter::once(base.clone())
        .chain((1..).map(|n| format!("{base}-{n}")))
        .find(|id| !store.exists(&dir.join(format!("{id}.{EXTENSION}"))))
        .unwrap_or(base);
    let context = SavedContext {
        deployment: deployment.to_owned(),
        id: id.clone(),
        saved_at: rfc3339(now_millis),
        label: label.map(str::to_owned),
        fail_on,
    };
    let mut content = Vec::new();
    render_sarif(report, tool_version, Some(&context), &mut content)?;
    let path = dir.join(format!("{id}.{EXTENSION}"));
    store.write_atomically(&path, &String::from_utf8_lossy(&content))?;
    Ok(path)
}

/// The saved reports of a deployment, most recent first.
pub fn list(store: &dyn Store, root: &Path, deployment: &str) -> Vec<Entry> {
    if !is_valid_id(deployment) {
        return Vec::new();
    }
    let dir = root.join(deployment);
    let mut ids: Vec<String> = store
        .files(&dir)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|name| {
            name.strip_suffix(&format!(".{EXTENSION}"))
                .filter(|id| is_valid_id(id))
                .map(str::to_owned)
        })
        .collect();
    ids.sort_by(|a, b| b.cmp(a));
    ids.into_iter()
        .map(|id| read_entry(store, &dir, id))
        .collect()
}

/// One saved report, if `id` names one.
pub fn read(store: &dyn Store, root: &Path, deployment: &str, id: &str) -> Option<Entry> {
    let dir = root.join(deployment);
    (is_valid_id(deployment)
        && is_valid_id(id)
        && store.exists(&dir.join(format!("{id}.{EXTENSION}"))))
    .then(|| read_entry(store, &dir, id.to_owned()))
}

/// Names of the deployments that have a history directory, sorted.
pub fn deployments(store: &dyn Store, root: &Path) -> Vec<String> {
    let mut names = store.dirs(root).unwrap_or_default();
    names.retain(|name| !name.starts_with('.'));
    names.sort();
    names
}

fn read_entry(store: &dyn Store, dir: &Path, id: String) -> Entry {
    let outcome = store
        .read(&dir.join(format!("{id}.{EXTENSION}")))
        .map_err(|error| error.to_string())
        .and_then(|text| parse_sarif(&text));
    Entry { id, outcome }
}

/// IDs come from URLs: letters, digits, `.` and `-` only, so that no ID can
/// leave the deployment directory.
fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemoryStore;
    use eyeship_domain::{FileId, Finding, Location, RuleId, Severity};

    const NOW: u64 = 1_764_157_637_123;
    const ROOT: &str = "/work/.eyeship/reports";

    fn report() -> Report {
        Report {
            files: vec!["compose.yaml".into()],
            findings: vec![Finding {
                rule: RuleId("R008"),
                severity: Severity::Warning,
                service: Some("api".into()),
                location: Location::new(FileId(0), 3, 12),
                message: "image is not pinned".into(),
                fix: "pin it".into(),
            }],
            ignored: Vec::new(),
        }
    }

    fn save_at(store: &MemoryStore, deployment: &str, now: u64, label: Option<&str>) -> PathBuf {
        save(
            store,
            Path::new(ROOT),
            &report(),
            &SaveRequest {
                deployment,
                fail_on: FailOn::Error,
                label,
                now_millis: now,
                tool_version: "0.1.0",
            },
        )
        .expect("saved")
    }

    #[test]
    fn saves_under_the_deployment_with_the_date_as_name() {
        let store = MemoryStore::default();
        let path = save_at(&store, "prod", NOW, Some("a1b2c3d"));
        assert_eq!(
            path,
            Path::new(ROOT).join("prod/2025-11-26T11-47-17.123Z.sarif")
        );
        let entry =
            read(&store, Path::new(ROOT), "prod", "2025-11-26T11-47-17.123Z").expect("found");
        let record = entry.outcome.expect("readable");
        let context = record.context.expect("saved context");
        assert_eq!(context.saved_at, "2025-11-26T11:47:17.123Z");
        assert_eq!(context.label.as_deref(), Some("a1b2c3d"));
        assert_eq!(context.deployment, "prod");
        assert_eq!(record.report.findings.len(), 1);
    }

    #[test]
    fn never_overwrites_a_report_saved_in_the_same_millisecond() {
        let store = MemoryStore::default();
        let first = save_at(&store, "prod", NOW, None);
        let second = save_at(&store, "prod", NOW, None);
        let third = save_at(&store, "prod", NOW, None);
        assert_ne!(first, second);
        assert!(second.ends_with("2025-11-26T11-47-17.123Z-1.sarif"));
        assert!(third.ends_with("2025-11-26T11-47-17.123Z-2.sarif"));
        assert_eq!(list(&store, Path::new(ROOT), "prod").len(), 3);
    }

    #[test]
    fn lists_the_most_recent_first_and_flags_unreadable_files() {
        let store = MemoryStore::new([
            (format!("{ROOT}/prod/2025-01-01T00-00-00.000Z.sarif"), "{"),
            (format!("{ROOT}/prod/notes.txt"), "not a report"),
        ]);
        save_at(&store, "prod", NOW, None);
        save_at(&store, "prod", NOW - 86_400_000, None);
        let entries = list(&store, Path::new(ROOT), "prod");
        let ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "2025-11-26T11-47-17.123Z",
                "2025-11-25T11-47-17.123Z",
                "2025-01-01T00-00-00.000Z"
            ]
        );
        assert!(entries[0].outcome.is_ok());
        assert!(entries[2].outcome.is_err());
        assert!(list(&store, Path::new(ROOT), "staging").is_empty());
    }

    #[test]
    fn ids_cannot_leave_the_deployment_directory() {
        let store = MemoryStore::new([(format!("{ROOT}/secret.sarif"), "{}")]);
        for id in ["../secret", "..", "", ".hidden", "a/b", "a\\b"] {
            assert!(read(&store, Path::new(ROOT), "prod", id).is_none(), "{id}");
        }
        let store = MemoryStore::new([(format!("{ROOT}/../x.sarif"), "{}")]);
        assert!(list(&store, Path::new(ROOT), "..").is_empty());
        assert!(read(&store, Path::new(ROOT), "..", "x").is_none());
    }

    #[test]
    fn lists_the_deployments_that_have_a_history() {
        let store = MemoryStore::default();
        save_at(&store, "staging", NOW, None);
        save_at(&store, "prod", NOW, None);
        assert_eq!(deployments(&store, Path::new(ROOT)), ["prod", "staging"]);
    }
}
