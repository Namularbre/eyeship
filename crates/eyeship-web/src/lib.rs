//! Local web interface (spec 004): the deployments seen by the user, the
//! routes, and the HTTP server at the edge.
#![forbid(unsafe_code)]

mod deployment;
mod handler;
mod route;
mod server;

pub use deployment::{DeploymentResult, sort_deployments};
pub use eyeship_report::Status;
pub use handler::{DeploymentSource, Request, Response, handle, is_local_host};
pub use route::{Route, route};
pub use server::{Bound, bind};
