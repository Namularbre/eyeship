//! Storage port of the history: a directory of text files.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The few file operations the history needs.
pub trait Store {
    /// Names of the files directly in `dir`; a missing directory is empty.
    fn files(&self, dir: &Path) -> io::Result<Vec<String>>;
    /// Names of the directories directly in `dir`; a missing directory is empty.
    fn dirs(&self, dir: &Path) -> io::Result<Vec<String>>;
    fn read(&self, path: &Path) -> io::Result<String>;
    fn exists(&self, path: &Path) -> bool;
    /// Writes a whole file at once: readers never see it half written.
    /// Creates the parent directories.
    fn write_atomically(&self, path: &Path, content: &str) -> io::Result<()>;
}

/// The real file system.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealStore;

fn names(dir: &Path, keep: impl Fn(&fs::FileType) -> bool) -> io::Result<Vec<String>> {
    match fs::read_dir(dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
        Ok(entries) => entries
            .filter_map(|entry| {
                entry
                    .and_then(|entry| {
                        Ok(keep(&entry.file_type()?)
                            .then(|| entry.file_name().to_string_lossy().into_owned()))
                    })
                    .transpose()
            })
            .collect(),
    }
}

impl Store for RealStore {
    fn files(&self, dir: &Path) -> io::Result<Vec<String>> {
        names(dir, fs::FileType::is_file)
    }

    fn dirs(&self, dir: &Path) -> io::Result<Vec<String>> {
        names(dir, fs::FileType::is_dir)
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn write_atomically(&self, path: &Path, content: &str) -> io::Result<()> {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(dir)?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let temporary = dir.join(format!(".{name}.tmp"));
        fs::write(&temporary, content)?;
        fs::rename(&temporary, path).inspect_err(|_| {
            // Best effort: do not leave the temporary file behind.
            let _ = fs::remove_file(&temporary);
        })
    }
}

/// An in-memory store for tests.
#[derive(Debug, Default)]
pub struct MemoryStore {
    files: RefCell<BTreeMap<PathBuf, String>>,
}

impl MemoryStore {
    pub fn new<I, P, C>(files: I) -> Self
    where
        I: IntoIterator<Item = (P, C)>,
        P: Into<PathBuf>,
        C: Into<String>,
    {
        Self {
            files: RefCell::new(
                files
                    .into_iter()
                    .map(|(path, content)| (path.into(), content.into()))
                    .collect(),
            ),
        }
    }

    /// Paths of every file, sorted.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.files.borrow().keys().cloned().collect()
    }

    fn children(&self, dir: &Path, directories: bool) -> Vec<String> {
        let mut names: Vec<String> = self
            .files
            .borrow()
            .keys()
            .filter_map(|path| path.strip_prefix(dir).ok())
            .filter_map(|relative| {
                let mut parts = relative.components();
                let first = parts.next()?.as_os_str().to_string_lossy().into_owned();
                (parts.next().is_some() == directories).then_some(first)
            })
            .collect();
        names.dedup();
        names
    }
}

impl Store for MemoryStore {
    fn files(&self, dir: &Path) -> io::Result<Vec<String>> {
        Ok(self.children(dir, false))
    }

    fn dirs(&self, dir: &Path) -> io::Result<Vec<String>> {
        Ok(self.children(dir, true))
    }

    fn read(&self, path: &Path) -> io::Result<String> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn exists(&self, path: &Path) -> bool {
        self.files
            .borrow()
            .keys()
            .any(|known| known.starts_with(path))
    }

    fn write_atomically(&self, path: &Path, content: &str) -> io::Result<()> {
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), content.to_owned());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(store: &dyn Store, root: &Path) {
        assert_eq!(store.files(&root.join("missing")).ok(), Some(Vec::new()));
        store
            .write_atomically(&root.join("prod/a.sarif"), "A")
            .expect("written");
        store
            .write_atomically(&root.join("prod/b.sarif"), "B")
            .expect("written");
        store
            .write_atomically(&root.join("staging/c.sarif"), "C")
            .expect("written");
        let mut files = store.files(&root.join("prod")).expect("listed");
        files.sort();
        assert_eq!(files, ["a.sarif", "b.sarif"]);
        let mut dirs = store.dirs(root).expect("listed");
        dirs.sort();
        assert_eq!(dirs, ["prod", "staging"]);
        assert!(store.files(root).expect("listed").is_empty());
        assert_eq!(
            store.read(&root.join("prod/b.sarif")).ok(),
            Some("B".into())
        );
        assert!(store.exists(&root.join("prod/a.sarif")));
        assert!(!store.exists(&root.join("prod/z.sarif")));
    }

    #[test]
    fn the_memory_store_behaves_like_a_directory() {
        check(&MemoryStore::default(), Path::new("/reports"));
    }

    #[test]
    fn the_real_store_writes_without_leftovers() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = dir.path().join(".eyeship/reports");
        check(&RealStore, &root);
        let leftovers: Vec<_> = fs::read_dir(root.join("prod"))
            .expect("listed")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
