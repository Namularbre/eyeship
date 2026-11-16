//! Small builders to write rule tests without YAML.
//!
//! Fields of the model are public: start from these defaults and use struct
//! update syntax, e.g. `Service { has_build: true, ..service("api", 3) }`.

use std::time::Duration;

use crate::finding::{FileId, Located, Location};
use crate::model::{
    Condition, Declared, Dependency, HostPath, HostPathKind, ImageRef, PortMapping, PortRange,
    Project, Protocol, Service,
};

/// A location in the first file, column 1.
pub fn at(line: u32) -> Location {
    Location::new(FileId(0), line, 1)
}

pub fn located<T>(value: T, line: u32) -> Located<T> {
    Located::new(value, at(line))
}

/// A project with the given services, in `compose.yaml`.
pub fn project(services: Vec<Service>) -> Project {
    Project {
        files: vec!["compose.yaml".into()],
        services,
        ..Project::default()
    }
}

/// An empty service declared at `line`.
pub fn service(name: &str, line: u32) -> Service {
    Service::new(name, at(line))
}

pub fn image(raw: &str, line: u32) -> Option<ImageRef> {
    Some(ImageRef::parse(raw, at(line)))
}

pub fn depends_on(name: &str, condition: Condition, line: u32) -> Dependency {
    Dependency {
        service: name.into(),
        condition,
        location: at(line),
    }
}

pub fn declared(name: &str, line: u32) -> Declared {
    Declared {
        name: name.into(),
        external: false,
        location: at(line),
    }
}

/// A TCP port published on every interface.
pub fn port(published: u16, target: u16, line: u32) -> PortMapping {
    PortMapping {
        host_ip: None,
        published: Some(PortRange::single(published)),
        target: PortRange::single(target),
        protocol: Protocol::Tcp,
        location: at(line),
    }
}

pub fn host_path(path: &str, kind: HostPathKind, exists: bool, line: u32) -> HostPath {
    HostPath {
        path: path.into(),
        kind,
        exists,
        location: at(line),
    }
}

pub fn seconds(value: u64) -> Duration {
    Duration::from_secs(value)
}
