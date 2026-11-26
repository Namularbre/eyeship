//! `eyeship ui`: the local web interface (spec 004 US2).

use std::io::Write;
use std::path::PathBuf;

use eyeship_history::{Entry, deployments, list};
use eyeship_web::{DeploymentResult, DeploymentSource, bind};

use crate::cli::UiArgs;
use crate::commands::{Context, USAGE_ERROR, report_error};
use crate::config::load_config;
use crate::pipeline::audit_deployments;

/// Reads the configuration and the files again on every request.
pub struct Source<'a> {
    pub context: &'a Context<'a>,
    /// `--reports-dir`, overriding the configuration.
    pub reports_dir: Option<PathBuf>,
}

impl Source<'_> {
    fn reports_root(&self) -> PathBuf {
        let relative = self.reports_dir.clone().unwrap_or_else(|| {
            load_config(self.context.fs, &self.context.working_dir)
                .map(|config| config.reports_dir())
                .unwrap_or_default()
        });
        self.context.working_dir.join(relative)
    }
}

impl DeploymentSource for Source<'_> {
    fn deployments(&self) -> Result<Vec<DeploymentResult>, String> {
        let context = self.context;
        load_config(context.fs, &context.working_dir)
            .map(|config| audit_deployments(&config, context.fs, context.env, &context.working_dir))
            .map_err(|error| error.to_string())
    }

    fn history_names(&self) -> Vec<String> {
        deployments(self.context.store, &self.reports_root())
    }

    fn history(&self, name: &str) -> Vec<Entry> {
        list(self.context.store, &self.reports_root(), name)
    }
}

/// Starts the server and serves until the process is stopped. Returns only
/// on a startup error.
pub fn run_ui(args: &UiArgs, context: &Context, err: &mut dyn Write) -> u8 {
    if let Err(error) = load_config(context.fs, &context.working_dir) {
        return report_error(err, &error.to_string());
    }
    let bound = match bind(args.host, args.port) {
        Ok(bound) => bound,
        Err(error) => {
            return report_error(
                err,
                &format!("cannot listen on {}:{}: {error}", args.host, args.port),
            );
        }
    };
    if !bound.is_local() {
        let _ = writeln!(
            err,
            "warning: listening on {}: anyone who can reach this address can read your reports",
            bound.address()
        );
    }
    let _ = writeln!(
        err,
        "eyeship: serving http://{} (Ctrl+C to stop)",
        bound.address()
    );
    let _ = err.flush();
    bound.serve(&Source {
        context,
        reports_dir: args.reports_dir.clone(),
    });
    USAGE_ERROR
}
