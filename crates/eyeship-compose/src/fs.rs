//! File system port: the only way the Compose adapter touches the disk.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::iter::once;
use std::path::{Path, PathBuf};

/// Read-only access to files, abstracted so the adapter is testable without a disk.
pub trait FileSystem {
    /// Reads the whole file at `path` as UTF-8 text.
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    /// Tells whether a file or directory exists at `path`.
    fn exists(&self, path: &Path) -> bool;
}

/// The real file system, backed by [`std::fs`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// An in-memory file system for tests.
///
/// A directory exists when it was added explicitly or when any file or
/// directory lies beneath it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryFileSystem {
    files: BTreeMap<PathBuf, String>,
    dirs: BTreeSet<PathBuf>,
}

impl MemoryFileSystem {
    /// Builds a file system from `(path, content)` pairs.
    pub fn new<I, P, C>(files: I) -> Self
    where
        I: IntoIterator<Item = (P, C)>,
        P: Into<PathBuf>,
        C: Into<String>,
    {
        Self {
            files: files
                .into_iter()
                .map(|(path, content)| (path.into(), content.into()))
                .collect(),
            dirs: BTreeSet::new(),
        }
    }

    /// Returns a copy with an (empty) directory added at `path`.
    #[must_use]
    pub fn with_dir(self, path: impl Into<PathBuf>) -> Self {
        Self {
            dirs: self.dirs.into_iter().chain(once(path.into())).collect(),
            ..self
        }
    }
}

impl FileSystem for MemoryFileSystem {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn exists(&self, path: &Path) -> bool {
        self.files
            .keys()
            .chain(self.dirs.iter())
            .any(|known| known.starts_with(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MemoryFileSystem {
        MemoryFileSystem::new([
            ("/app/compose.yaml", "services: {}"),
            ("/app/config/nginx.conf", "events {}"),
        ])
    }

    #[test]
    fn memory_reads_known_file() {
        let fs = sample();
        assert_eq!(
            fs.read_to_string(Path::new("/app/compose.yaml")).ok(),
            Some("services: {}".to_owned())
        );
    }

    #[test]
    fn memory_read_of_unknown_file_is_not_found() {
        let err = sample().read_to_string(Path::new("/app/missing.yaml"));
        assert_eq!(err.map_err(|e| e.kind()), Err(io::ErrorKind::NotFound));
    }

    #[test]
    fn memory_files_exist() {
        assert!(sample().exists(Path::new("/app/config/nginx.conf")));
    }

    #[test]
    fn memory_parent_directories_exist() {
        let fs = sample();
        assert!(fs.exists(Path::new("/app/config")));
        assert!(fs.exists(Path::new("/app")));
        assert!(fs.exists(Path::new("/")));
    }

    #[test]
    fn memory_unknown_paths_do_not_exist() {
        let fs = sample();
        assert!(!fs.exists(Path::new("/app/data")));
        // A shared string prefix is not a parent directory.
        assert!(!fs.exists(Path::new("/app/conf")));
    }

    #[test]
    fn memory_explicit_empty_directory_exists() {
        let fs = sample().with_dir("/app/data");
        assert!(fs.exists(Path::new("/app/data")));
        assert!(fs.read_to_string(Path::new("/app/data")).is_err());
    }

    #[test]
    fn memory_parent_of_explicit_directory_exists() {
        let fs = MemoryFileSystem::default().with_dir("/srv/volumes/db");
        assert!(fs.exists(Path::new("/srv/volumes")));
    }

    #[test]
    fn real_file_system_sees_this_crate_manifest() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let fs = RealFileSystem;
        assert!(fs.exists(&manifest));
        assert!(
            fs.read_to_string(&manifest)
                .is_ok_and(|s| s.contains("eyeship-compose"))
        );
        assert!(!fs.exists(&manifest.with_file_name("no-such-file")));
    }
}
