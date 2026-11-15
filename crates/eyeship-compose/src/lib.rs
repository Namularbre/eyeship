//! Docker Compose input adapter: turns Compose files into an
//! [`eyeship_domain`] project, with interpolation and facts from the disk.
#![forbid(unsafe_code)]

mod context;
mod depends_on;
pub mod discovery;
pub mod dotenv;
mod duration;
pub mod env;
mod error;
mod files;
pub mod fs;
pub mod interpolate;
mod loader;
mod location;
mod merge;
mod networks;
mod ports;
mod raw;
mod resources;
mod service;
mod size;
mod unresolved;
pub mod vars;
mod volumes;

pub use discovery::{DEFAULT_FILE_NAMES, discover};
pub use dotenv::parse_dotenv;
pub use env::{Env, MemoryEnv, ProcessEnv};
pub use error::LoadError;
pub use fs::{FileSystem, MemoryFileSystem, RealFileSystem};
pub use interpolate::{Interpolated, Unresolved, interpolate};
pub use loader::{LoadRequest, load};
pub use vars::VarSource;
