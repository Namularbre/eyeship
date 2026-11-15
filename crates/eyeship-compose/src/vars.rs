//! Variable source for interpolation: process environment, then `.env`.

use std::collections::BTreeMap;

use crate::env::Env;

/// Resolves variables like Docker Compose: the process environment wins over
/// the `.env` file.
pub struct VarSource<'a> {
    env: &'a dyn Env,
    dotenv: BTreeMap<String, String>,
}

impl<'a> VarSource<'a> {
    /// Combines a process environment with `.env` pairs (later pairs win).
    pub fn new(env: &'a dyn Env, dotenv: Vec<(String, String)>) -> Self {
        Self {
            env,
            dotenv: dotenv.into_iter().collect(),
        }
    }

    /// Returns the value of `name`, from the environment first, then `.env`.
    pub fn lookup(&self, name: &str) -> Option<String> {
        self.env
            .var(name)
            .or_else(|| self.dotenv.get(name).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dotenv::parse_dotenv;
    use crate::env::MemoryEnv;
    use crate::interpolate::interpolate;

    #[test]
    fn process_env_wins_over_dotenv() {
        let env = MemoryEnv::new([("TAG", "from-env"), ("EMPTY", "")]);
        let vars = VarSource::new(&env, parse_dotenv("TAG=from-file\nEMPTY=x\nONLY=file"));
        assert_eq!(vars.lookup("TAG"), Some("from-env".to_owned()));
        assert_eq!(vars.lookup("EMPTY"), Some(String::new()));
        assert_eq!(vars.lookup("ONLY"), Some("file".to_owned()));
        assert_eq!(vars.lookup("MISSING"), None);
    }

    #[test]
    fn later_dotenv_pairs_win() {
        let env = MemoryEnv::default();
        let vars = VarSource::new(&env, parse_dotenv("A=1\nA=2"));
        assert_eq!(vars.lookup("A"), Some("2".to_owned()));
    }

    #[test]
    fn feeds_interpolation() {
        let env = MemoryEnv::new([("TAG", "1.2")]);
        let vars = VarSource::new(&env, parse_dotenv("IMAGE=nginx"));
        let lookup = |name: &str| vars.lookup(name);
        assert_eq!(interpolate("${IMAGE}:$TAG", &lookup).value, "nginx:1.2");
    }
}
