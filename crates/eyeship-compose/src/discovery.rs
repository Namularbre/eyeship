//! Discovery of the default Compose file in a directory.

use std::path::{Path, PathBuf};

use crate::fs::FileSystem;

/// Default Compose file names, in Docker Compose priority order.
pub const DEFAULT_FILE_NAMES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// Returns the first default Compose file that exists in `dir`.
pub fn discover(dir: &Path, fs: &dyn FileSystem) -> Option<PathBuf> {
    DEFAULT_FILE_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| fs.exists(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemoryFileSystem;

    fn fs_with(names: &[&str]) -> MemoryFileSystem {
        MemoryFileSystem::new(names.iter().map(|n| (Path::new("/p").join(n), "")))
    }

    #[test]
    fn finds_nothing_in_empty_directory() {
        let fs = MemoryFileSystem::default().with_dir("/p");
        assert_eq!(discover(Path::new("/p"), &fs), None);
    }

    #[test]
    fn finds_each_name_alone() {
        DEFAULT_FILE_NAMES.iter().for_each(|name| {
            let fs = fs_with(&[name]);
            assert_eq!(
                discover(Path::new("/p"), &fs),
                Some(PathBuf::from("/p").join(name))
            );
        });
    }

    #[test]
    fn prefers_compose_yaml_over_everything() {
        let fs = fs_with(&DEFAULT_FILE_NAMES);
        assert_eq!(
            discover(Path::new("/p"), &fs),
            Some(PathBuf::from("/p/compose.yaml"))
        );
    }

    #[test]
    fn prefers_compose_yml_over_docker_compose() {
        let fs = fs_with(&["docker-compose.yaml", "compose.yml"]);
        assert_eq!(
            discover(Path::new("/p"), &fs),
            Some(PathBuf::from("/p/compose.yml"))
        );
    }

    #[test]
    fn prefers_docker_compose_yaml_over_yml() {
        let fs = fs_with(&["docker-compose.yml", "docker-compose.yaml"]);
        assert_eq!(
            discover(Path::new("/p"), &fs),
            Some(PathBuf::from("/p/docker-compose.yaml"))
        );
    }

    #[test]
    fn ignores_other_files_and_other_directories() {
        let fs = MemoryFileSystem::new([("/p/compose.json", ""), ("/q/compose.yaml", "")]);
        assert_eq!(discover(Path::new("/p"), &fs), None);
    }
}
