//! Docker Compose input adapter: turns Compose files into an
//! [`eyeship_domain`] project, with interpolation and facts from the disk.
#![forbid(unsafe_code)]

pub mod discovery;
pub mod dotenv;
pub mod env;
pub mod fs;
pub mod interpolate;
pub mod vars;

pub use discovery::{DEFAULT_FILE_NAMES, discover};
pub use dotenv::parse_dotenv;
pub use env::{Env, MemoryEnv, ProcessEnv};
pub use fs::{FileSystem, MemoryFileSystem, RealFileSystem};
pub use interpolate::{Interpolated, Unresolved, interpolate};
pub use vars::VarSource;
