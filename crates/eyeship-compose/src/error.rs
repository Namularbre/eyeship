//! Errors that prevent loading a Compose project.

use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::discovery::DEFAULT_FILE_NAMES;

/// Why a Compose project could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    /// The given file (Compose file or env file) does not exist.
    NotFound { path: PathBuf },
    /// The file exists but could not be read.
    Io { path: PathBuf, source: io::Error },
    /// The file is not valid YAML, or not a valid Compose structure.
    InvalidYaml {
        path: PathBuf,
        message: String,
        line: u32,
        column: u32,
    },
    /// No default Compose file in the directory.
    NoComposeFile { dir: PathBuf },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { path } => write!(
                f,
                "file not found: {}. Check the path and try again.",
                path.display()
            ),
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::InvalidYaml {
                path,
                message,
                line,
                column,
            } => write!(
                f,
                "invalid YAML in {}:{line}:{column}: {message}",
                path.display()
            ),
            Self::NoComposeFile { dir } => write!(
                f,
                "no Compose file found in {}: expected one of {}. Run eyeship from the project directory or pass the file path.",
                dir.display(),
                DEFAULT_FILE_NAMES.join(", ")
            ),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_name_the_file_and_the_position() {
        let not_found = LoadError::NotFound {
            path: "compose.yaml".into(),
        };
        assert_eq!(
            not_found.to_string(),
            "file not found: compose.yaml. Check the path and try again."
        );
        let invalid = LoadError::InvalidYaml {
            path: "compose.yaml".into(),
            message: "expected mapping start".into(),
            line: 3,
            column: 7,
        };
        assert_eq!(
            invalid.to_string(),
            "invalid YAML in compose.yaml:3:7: expected mapping start"
        );
    }

    #[test]
    fn no_compose_file_lists_the_expected_names() {
        let message = LoadError::NoComposeFile { dir: "/p".into() }.to_string();
        assert!(message.starts_with("no Compose file found in /p: expected one of compose.yaml,"));
        assert!(message.contains("docker-compose.yml"));
    }

    #[test]
    fn io_errors_expose_their_source() {
        let error = LoadError::Io {
            path: "c.yaml".into(),
            source: io::Error::other("boom"),
        };
        assert_eq!(error.to_string(), "cannot read c.yaml: boom");
        assert!(error.source().is_some());
    }
}
