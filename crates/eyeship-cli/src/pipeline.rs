//! What to audit and how, shared by `compose`, `--deployment` and `ui` so
//! that they give the same result by construction (spec 004 US5 §2).

use std::path::{Path, PathBuf};

use eyeship_compose::{Env, FileSystem, LoadError, LoadRequest, load};
use eyeship_domain::{AuditConfig, FailOn, RULES, Report, audit};
use eyeship_web::{DeploymentResult, sort_deployments};

use crate::config::{Config, Deployment};

/// The files and overrides of one audit, before the configuration applies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    pub files: Vec<PathBuf>,
    pub env_file: Option<PathBuf>,
    /// Threshold set for this audit (command line or deployment).
    pub fail_on: Option<FailOn>,
    /// Rule IDs ignored for this audit only, added to the configuration's.
    pub ignore: Vec<String>,
}

impl From<&Deployment> for Target {
    fn from(deployment: &Deployment) -> Self {
        Self {
            files: deployment.files.clone(),
            env_file: deployment.env_file.clone(),
            fail_on: deployment.fail_on,
            ignore: Vec::new(),
        }
    }
}

/// Everything needed to run one audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditPlan {
    pub request: LoadRequest,
    pub audit_config: AuditConfig,
    pub fail_on: FailOn,
}

/// Combines a target with the configuration: the target's threshold wins
/// over the configuration's, and ignored rules add up.
pub fn plan(target: &Target, config: &Config, working_dir: &Path) -> AuditPlan {
    AuditPlan {
        request: LoadRequest {
            files: target.files.clone(),
            env_file: target.env_file.clone(),
            working_dir: working_dir.to_path_buf(),
        },
        audit_config: AuditConfig {
            ignored_rules: config
                .ignored_rules
                .iter()
                .cloned()
                .chain(target.ignore.iter().map(|id| id.trim().to_uppercase()))
                .collect(),
            ignored_for_service: config.ignored_for_service.clone(),
        },
        fail_on: target.fail_on.or(config.fail_on).unwrap_or_default(),
    }
}

/// Loads the project and audits it with every rule.
pub fn execute(plan: &AuditPlan, fs: &dyn FileSystem, env: &dyn Env) -> Result<Report, LoadError> {
    load(&plan.request, fs, env).map(|project| audit(&project, RULES, &plan.audit_config))
}

/// Audits every deployment of the configuration; one that cannot be read is
/// reported as such without hiding the others (spec 004 US1 §3).
pub fn audit_deployments(
    config: &Config,
    fs: &dyn FileSystem,
    env: &dyn Env,
    working_dir: &Path,
) -> Vec<DeploymentResult> {
    sort_deployments(
        config
            .effective_deployments()
            .iter()
            .map(|deployment| audit_deployment(deployment, config, fs, env, working_dir))
            .collect(),
    )
}

/// Audits one deployment.
pub fn audit_deployment(
    deployment: &Deployment,
    config: &Config,
    fs: &dyn FileSystem,
    env: &dyn Env,
    working_dir: &Path,
) -> DeploymentResult {
    let plan = plan(&Target::from(deployment), config, working_dir);
    let outcome = execute(&plan, fs, env).map_err(|error| error.to_string());
    let files = match &outcome {
        Ok(report) => report.files.clone(),
        Err(_) => deployment
            .files
            .iter()
            .map(|file| file.display().to_string())
            .collect(),
    };
    DeploymentResult {
        name: deployment.name.clone(),
        files,
        fail_on: plan.fail_on,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyeship_compose::{MemoryEnv, MemoryFileSystem};
    use eyeship_web::Status;

    use crate::config::parse_config;

    const CLEAN: &str = "services:\n  web:\n    image: nginx:1.27\n    restart: unless-stopped\n    mem_limit: 256m\n    logging:\n      driver: json-file\n      options:\n        max-size: 10m\n";
    const CONFLICT: &str = "services:\n  a:\n    image: nginx:1.27\n    ports: [\"80:80\"]\n  b:\n    image: nginx:1.27\n    ports: [\"80:8080\"]\n";

    fn no_env() -> MemoryEnv {
        MemoryEnv::new(Vec::<(String, String)>::new())
    }

    #[test]
    fn the_target_threshold_wins_and_ignores_add_up() {
        let config = parse_config("fail_on = \"warning\"\nignore = [\"R001\"]\n").expect("valid");
        let target = Target {
            fail_on: Some(FailOn::None),
            ignore: vec![" r002 ".into()],
            ..Target::default()
        };
        let plan = plan(&target, &config, Path::new("/work"));
        assert_eq!(plan.fail_on, FailOn::None);
        assert_eq!(plan.audit_config.ignored_rules, ["R001", "R002"]);
        assert_eq!(plan.request.working_dir, PathBuf::from("/work"));
    }

    #[test]
    fn without_a_target_threshold_the_configuration_applies() {
        let config = parse_config("fail_on = \"info\"\n").expect("valid");
        let plan = plan(&Target::default(), &config, Path::new("/work"));
        assert_eq!(plan.fail_on, FailOn::Info);
        let default = super::plan(&Target::default(), &Config::default(), Path::new("/work"));
        assert_eq!(default.fail_on, FailOn::Error);
    }

    #[test]
    fn audits_every_deployment_and_reports_unreadable_ones() {
        let fs = MemoryFileSystem::new([
            ("/work/compose.yaml", CLEAN),
            ("/work/compose.prod.yaml", CONFLICT),
        ]);
        let config = parse_config(
            "[deployments.staging]\nfiles = [\"compose.yaml\"]\n\
             [deployments.prod]\nfiles = [\"compose.prod.yaml\"]\n\
             [deployments.dev]\nfiles = [\"missing.yaml\"]\n",
        )
        .expect("valid");
        let results = audit_deployments(&config, &fs, &no_env(), Path::new("/work"));
        let summary: Vec<(&str, Status)> = results
            .iter()
            .map(|result| (result.name.as_str(), result.status()))
            .collect();
        assert_eq!(
            summary,
            [
                ("dev", Status::Unreadable),
                ("prod", Status::Fail),
                ("staging", Status::Pass)
            ]
        );
        assert_eq!(results[0].files, ["missing.yaml"]);
        assert!(
            results[0]
                .outcome
                .as_ref()
                .is_err_and(|e| e.contains("missing.yaml"))
        );
        assert_eq!(results[2].files, ["compose.yaml"]);
    }

    #[test]
    fn a_deployment_threshold_applies_to_it_only() {
        let fs = MemoryFileSystem::new([("/work/compose.yaml", CONFLICT)]);
        let config = parse_config(
            "[deployments.lenient]\nfiles = [\"compose.yaml\"]\nfail_on = \"none\"\n\
             [deployments.strict]\nfiles = [\"compose.yaml\"]\n",
        )
        .expect("valid");
        let results = audit_deployments(&config, &fs, &no_env(), Path::new("/work"));
        let statuses: Vec<Status> = results.iter().map(DeploymentResult::status).collect();
        assert_eq!(statuses, [Status::Fail, Status::Pass]);
        assert_eq!(results[0].name, "strict");
    }

    #[test]
    fn the_implicit_deployment_discovers_the_default_file() {
        let fs = MemoryFileSystem::new([("/work/compose.yaml", CLEAN)]);
        let results = audit_deployments(&Config::default(), &fs, &no_env(), Path::new("/work"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "default");
        assert_eq!(results[0].status(), Status::Pass);
    }
}
