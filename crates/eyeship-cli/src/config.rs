//! The optional `.eyeship.toml` configuration file (US4).
//!
//! ```toml
//! fail_on = "warning"          # optional
//! ignore = ["R015", "R016"]    # optional, every service
//! [services.api]
//! ignore = ["R006"]            # one service only
//! ```
//!
//! Unknown keys, unknown thresholds and unknown rule IDs are errors, so a
//! typo is reported instead of silently ignoring nothing.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use eyeship_compose::FileSystem;
use eyeship_domain::{FailOn, rules};
use serde::Deserialize;

/// Name of the configuration file.
pub const FILE_NAME: &str = ".eyeship.toml";

/// A validated configuration: canonical rule IDs and a parsed threshold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub fail_on: Option<FailOn>,
    pub ignored_rules: Vec<String>,
    pub ignored_for_service: Vec<(String, String)>,
}

/// A configuration file that cannot be read or is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub path: PathBuf,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid configuration file {}: {}",
            self.path.display(),
            self.message
        )
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    fail_on: Option<String>,
    #[serde(default)]
    ignore: Vec<String>,
    #[serde(default)]
    services: BTreeMap<String, RawService>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawService {
    #[serde(default)]
    ignore: Vec<String>,
}

/// Reads `.eyeship.toml` from `dir`, the directory where eyeship runs
/// (not the audited file's directory, so one file governs every audit
/// started from a project root). A missing file is an empty configuration.
pub fn load_config(fs: &dyn FileSystem, dir: &Path) -> Result<Config, ConfigError> {
    let path = dir.join(FILE_NAME);
    if !fs.exists(&path) {
        return Ok(Config::default());
    }
    fs.read_to_string(&path)
        .map_err(|error| error.to_string())
        .and_then(|content| parse_config(&content))
        .map_err(|message| ConfigError { path, message })
}

/// Parses and validates the content of a configuration file.
pub fn parse_config(content: &str) -> Result<Config, String> {
    let raw: RawConfig = toml::from_str(content).map_err(|error| error.to_string())?;
    let fail_on = raw.fail_on.as_deref().map(str::parse).transpose()?;
    let ignored_rules = canonical_ids(&raw.ignore)?;
    let ignored_for_service = raw
        .services
        .iter()
        .map(|(service, settings)| {
            canonical_ids(&settings.ignore)
                .map(|ids| ids.into_iter().map(|id| (service.clone(), id)).collect())
        })
        .collect::<Result<Vec<Vec<_>>, _>>()?
        .concat();
    Ok(Config {
        fail_on,
        ignored_rules,
        ignored_for_service,
    })
}

fn canonical_ids(ids: &[String]) -> Result<Vec<String>, String> {
    ids.iter()
        .map(|id| {
            rules::find(id.trim())
                .map(|rule| rule.id.as_str().to_owned())
                .ok_or_else(|| {
                    format!("unknown rule '{id}' (run `eyeship rules` to list the rules)")
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyeship_compose::MemoryFileSystem;

    #[test]
    fn an_empty_file_is_the_default_configuration() {
        assert_eq!(parse_config(""), Ok(Config::default()));
    }

    #[test]
    fn parses_every_setting() {
        let config = parse_config(
            "fail_on = \"warning\"\nignore = [\"R015\", \"r016\"]\n[services.api]\nignore = [\"R006\"]\n",
        );
        assert_eq!(
            config,
            Ok(Config {
                fail_on: Some(FailOn::Warning),
                ignored_rules: vec!["R015".into(), "R016".into()],
                ignored_for_service: vec![("api".into(), "R006".into())],
            })
        );
    }

    #[test]
    fn rejects_invalid_toml() {
        assert!(parse_config("ignore = [").is_err());
    }

    #[test]
    fn rejects_an_unknown_rule() {
        let error = parse_config("[services.api]\nignore = [\"R999\"]\n").unwrap_err();
        assert!(error.contains("unknown rule 'R999'"), "{error}");
    }

    #[test]
    fn rejects_an_unknown_threshold() {
        let error = parse_config("fail_on = \"loud\"\n").unwrap_err();
        assert!(error.contains("loud"), "{error}");
    }

    #[test]
    fn rejects_an_unknown_key() {
        assert!(parse_config("ignored = [\"R001\"]\n").is_err());
    }

    #[test]
    fn a_missing_file_is_the_default_configuration() {
        let fs = MemoryFileSystem::default();
        assert_eq!(load_config(&fs, Path::new("/work")), Ok(Config::default()));
    }

    #[test]
    fn loads_the_file_from_the_given_directory() {
        let fs = MemoryFileSystem::new([("/work/.eyeship.toml", "ignore = [\"R001\"]\n")]);
        assert_eq!(
            load_config(&fs, Path::new("/work")).map(|config| config.ignored_rules),
            Ok(vec!["R001".into()])
        );
    }

    #[test]
    fn errors_name_the_file() {
        let fs = MemoryFileSystem::new([("/work/.eyeship.toml", "ignore = [")]);
        let error = load_config(&fs, Path::new("/work")).unwrap_err();
        assert_eq!(error.path, PathBuf::from("/work/.eyeship.toml"));
        let expected = format!(
            "invalid configuration file {}: ",
            Path::new("/work").join(".eyeship.toml").display()
        );
        assert!(error.to_string().starts_with(&expected), "{error}");
    }
}
