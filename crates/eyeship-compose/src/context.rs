//! What the mapping needs from the outside world: variables and the disk.

use std::path::{Component, Path, PathBuf};

use eyeship_domain::{FileId, HostPath, HostPathKind, Located, Location};

use crate::fs::FileSystem;
use crate::interpolate::interpolate;
use crate::location::to_location;
use crate::raw::Text;
use crate::vars::VarSource;

/// Read-only context shared by every mapping function.
pub(crate) struct Context<'a> {
    pub file: FileId,
    /// Directory of the Compose file, against which relative paths resolve.
    pub dir: PathBuf,
    pub fs: &'a dyn FileSystem,
    pub vars: &'a VarSource<'a>,
}

impl Context<'_> {
    /// The domain location of a YAML position in the current file.
    pub fn location(&self, position: &serde_saphyr::Location) -> Location {
        to_location(self.file, position)
    }

    /// Interpolates a string; unresolved variables become empty.
    pub fn interpolate(&self, raw: &str) -> String {
        interpolate(raw, &|name| self.vars.lookup(name)).value
    }

    /// Whether `raw` references a variable with no value and no default.
    pub fn has_unresolved(&self, raw: &str) -> bool {
        !interpolate(raw, &|name| self.vars.lookup(name))
            .unresolved
            .is_empty()
    }

    /// Like [`Context::text`], but `None` when an unresolved variable broke
    /// the value: R013 reports the variable, rules must not judge the rest.
    pub fn resolved_text(&self, text: &Text) -> Option<Located<String>> {
        (!self.has_unresolved(text.value.as_str())).then(|| self.text(text))
    }

    /// Interpolates a located scalar, keeping its location.
    pub fn text(&self, text: &Text) -> Located<String> {
        Located::new(
            self.interpolate(text.value.as_str()),
            self.location(&text.referenced),
        )
    }

    /// A host path with whether it exists.
    ///
    /// Relative paths are checked against the Compose file's directory.
    /// Absolute and `~` paths are assumed to exist: they live on the target
    /// host, which we cannot see.
    pub fn host_path(&self, path: String, kind: HostPathKind, location: Location) -> HostPath {
        let exists = is_host_absolute(&path) || self.fs.exists(&normalize(&self.dir.join(&path)));
        HostPath {
            path,
            kind,
            exists,
            location,
        }
    }
}

/// Whether a path is absolute on the target host (Unix, `~` or Windows).
pub(crate) fn is_host_absolute(path: &str) -> bool {
    path.starts_with(['/', '\\', '~']) || has_windows_drive(path)
}

/// Whether a path starts with a Windows drive such as `C:\` or `c:/`.
pub(crate) fn has_windows_drive(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

/// Removes `.` and folds `..` lexically, without touching the disk.
fn normalize(path: &Path) -> PathBuf {
    path.components()
        .fold(PathBuf::new(), |acc, component| match component {
            Component::CurDir => acc,
            Component::ParentDir => acc.parent().map(Path::to_path_buf).unwrap_or(acc),
            other => acc.join(other),
        })
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Helpers to run mapping functions on in-memory YAML.

    use super::*;
    use crate::env::MemoryEnv;
    use crate::fs::MemoryFileSystem;
    use crate::raw::RawService;

    /// The directory of the Compose file in tests.
    pub const DIR: &str = "/app";

    /// Parses the body of one service.
    pub fn service(yaml: &str) -> RawService {
        serde_saphyr::from_str(yaml).unwrap()
    }

    /// Runs `f` with a context over `files` and the variables `vars`.
    pub fn with_context<R>(
        files: &[&str],
        vars: &[(&str, &str)],
        f: impl FnOnce(&Context<'_>) -> R,
    ) -> R {
        let fs = MemoryFileSystem::new(files.iter().map(|path| (*path, "")));
        let env = MemoryEnv::new(vars.iter().copied());
        let vars = VarSource::new(&env, Vec::new());
        f(&Context {
            file: FileId(0),
            dir: PathBuf::from(DIR),
            fs: &fs,
            vars: &vars,
        })
    }

    pub fn at(line: u32, column: u32) -> Location {
        Location::new(FileId(0), line, column)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    fn exists(files: &[&str], path: &str) -> bool {
        with_context(files, &[], |ctx| {
            ctx.host_path(path.to_owned(), HostPathKind::Bind, at(1, 1))
                .exists
        })
    }

    #[test]
    fn relative_paths_resolve_against_the_compose_directory() {
        assert!(exists(&["/app/data/x"], "./data"));
        assert!(exists(&["/app/data/x"], "data"));
        assert!(!exists(&["/app/data/x"], "./missing"));
    }

    #[test]
    fn parent_directories_are_folded() {
        assert!(exists(&["/shared/conf"], "../shared/conf"));
        assert!(!exists(&["/app/shared/conf"], "../shared/conf"));
    }

    #[test]
    fn absolute_and_home_paths_are_assumed_to_exist() {
        assert!(exists(&[], "/srv/data"));
        assert!(exists(&[], "~/data"));
        assert!(exists(&[], "C:\\data"));
    }

    #[test]
    fn text_is_interpolated_and_located() {
        let yaml = "image: nginx:${TAG}\n";
        let located = with_context(&[], &[("TAG", "1.2")], |ctx| {
            ctx.text(service(yaml).image.as_ref().unwrap())
        });
        assert_eq!(located, Located::new("nginx:1.2".to_owned(), at(1, 8)));
    }
}
