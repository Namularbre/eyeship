//! Local web interface (spec 004): the deployments seen by the user, the
//! routes, and the HTTP server at the edge.
#![forbid(unsafe_code)]

mod deployment;

pub use deployment::{DeploymentResult, sort_deployments};
pub use eyeship_report::Status;
