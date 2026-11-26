//! Pure core of eyeship: the deployment model, the rules, the audit engine
//! and the findings it produces.
//!
//! This crate knows nothing about YAML, the file system or the terminal.
#![forbid(unsafe_code)]

mod diff;
mod engine;
mod finding;
mod model;
mod rule;
pub mod rules;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use diff::{Diff, diff};
pub use engine::{AuditConfig, FailOn, Report, Summary, audit};
pub use finding::{FileId, Finding, Located, Location, RuleId, Severity};
pub use model::{
    Condition, Declared, Dependency, Healthcheck, HostPath, HostPathKind, ImageRef, Logging, Mount,
    PortMapping, PortRange, Project, Protocol, RestartPolicy, Service, UnresolvedVar,
};
pub use rule::{Rule, RuleDoc};
pub use rules::RULES;
