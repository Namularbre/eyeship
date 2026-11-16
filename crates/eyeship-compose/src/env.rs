//! Environment port: access to environment variables.

use std::collections::BTreeMap;

/// Read-only access to environment variables.
pub trait Env {
    /// Returns the value of `name`, or `None` when unset (or not valid Unicode).
    fn var(&self, name: &str) -> Option<String>;
}

/// The environment of the current process.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnv;

impl Env for ProcessEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

/// An in-memory environment for tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryEnv {
    vars: BTreeMap<String, String>,
}

impl MemoryEnv {
    /// Builds an environment from `(name, value)` pairs; later pairs win.
    pub fn new<I, K, V>(vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            vars: vars
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        }
    }
}

impl Env for MemoryEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_env_returns_known_values() {
        let env = MemoryEnv::new([("TAG", "1.2"), ("EMPTY", "")]);
        assert_eq!(env.var("TAG"), Some("1.2".to_owned()));
        assert_eq!(env.var("EMPTY"), Some(String::new()));
        assert_eq!(env.var("MISSING"), None);
    }

    #[test]
    fn memory_env_later_pairs_win() {
        let env = MemoryEnv::new([("A", "1"), ("A", "2")]);
        assert_eq!(env.var("A"), Some("2".to_owned()));
    }

    #[test]
    fn process_env_reads_cargo_variables() {
        // Cargo sets CARGO_PKG_NAME for test binaries at run time.
        assert_eq!(
            ProcessEnv.var("CARGO_PKG_NAME"),
            Some("eyeship-compose".to_owned())
        );
        assert_eq!(ProcessEnv.var("EYESHIP_SURELY_UNSET_VARIABLE"), None);
    }
}
