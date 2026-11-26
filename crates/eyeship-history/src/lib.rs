//! History of saved reports (spec 004 US7, US8; plan §4): one SARIF file per
//! audit in `<reports_dir>/<deployment>/<id>.sarif`. Keeping, archiving or
//! deleting these files is left to the user.
#![forbid(unsafe_code)]

mod clock;
mod history;
mod store;

pub use clock::{Clock, FixedClock, SystemClock, file_id, rfc3339};
pub use history::{Entry, SaveRequest, deployments, list, read, save};
pub use store::{MemoryStore, RealStore, Store};
