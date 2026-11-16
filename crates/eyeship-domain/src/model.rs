//! Deployment model, independent of any input format.
//!
//! Input adapters fill it with already-resolved facts (does a path exist,
//! is a variable defined) so that rules stay pure functions.

use std::time::Duration;

use crate::finding::{Located, Location};

/// A whole deployment: services plus the resources declared at the root.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Project {
    /// Source files, indexed by [`crate::FileId`].
    pub files: Vec<String>,
    pub services: Vec<Service>,
    /// Networks declared at the root.
    pub networks: Vec<Declared>,
    /// Named volumes declared at the root.
    pub volumes: Vec<Declared>,
    /// Location of the obsolete root `version` key, if present.
    pub has_version_key: Option<Location>,
    /// Variables referenced without default and absent from every source.
    pub unresolved_vars: Vec<UnresolvedVar>,
}

impl Project {
    /// Finds a service by name.
    pub fn service(&self, name: &str) -> Option<&Service> {
        self.services.iter().find(|service| service.name == name)
    }
}

/// A resource declared at the root of the project (network, volume).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub name: String,
    /// Managed outside the project (`external: true`).
    pub external: bool,
    pub location: Location,
}

/// A variable that could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedVar {
    pub name: String,
    /// Service in which the variable appears, if any.
    pub service: Option<String>,
    pub location: Location,
}

/// One service (container template) of the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    pub location: Location,
    pub image: Option<ImageRef>,
    pub has_build: bool,
    pub container_name: Option<Located<String>>,
    pub restart: Option<Located<RestartPolicy>>,
    /// Memory limit in bytes.
    pub memory_limit: Option<Located<u64>>,
    pub logging: Option<Logging>,
    pub healthcheck: Option<Healthcheck>,
    pub depends_on: Vec<Dependency>,
    pub ports: Vec<PortMapping>,
    pub networks: Vec<Located<String>>,
    pub mounts: Vec<Mount>,
    /// Other host files used by the service (`env_file`, file secrets and configs).
    pub files: Vec<HostPath>,
}

impl Service {
    /// A service with only a name and a location, everything else empty.
    pub fn new(name: impl Into<String>, location: Location) -> Self {
        Self {
            name: name.into(),
            location,
            image: None,
            has_build: false,
            container_name: None,
            restart: None,
            memory_limit: None,
            logging: None,
            healthcheck: None,
            depends_on: Vec::new(),
            ports: Vec::new(),
            networks: Vec::new(),
            mounts: Vec::new(),
            files: Vec::new(),
        }
    }
}

/// A parsed image reference: `[registry/]name[:tag][@digest]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    /// The reference exactly as written (after interpolation).
    pub raw: String,
    pub registry: Option<String>,
    pub name: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
    pub location: Location,
}

impl ImageRef {
    /// Splits an image reference into its parts. Never fails: an odd
    /// reference simply ends up in `name`.
    pub fn parse(raw: &str, location: Location) -> Self {
        let (without_digest, digest) = match raw.split_once('@') {
            Some((head, digest)) => (head, Some(digest.to_owned())),
            None => (raw, None),
        };
        let (registry, path) = match without_digest.split_once('/') {
            Some((first, rest)) if is_registry(first) => (Some(first.to_owned()), rest),
            _ => (None, without_digest),
        };
        let last_slash = path.rfind('/').map_or(0, |index| index + 1);
        let (name, tag) = match path[last_slash..].rfind(':') {
            Some(colon) => {
                let split = last_slash + colon;
                (&path[..split], Some(path[split + 1..].to_owned()))
            }
            None => (path, None),
        };
        Self {
            raw: raw.to_owned(),
            registry,
            name: name.to_owned(),
            tag,
            digest,
            location,
        }
    }
}

fn is_registry(component: &str) -> bool {
    component.contains('.') || component.contains(':') || component == "localhost"
}

/// The `restart` policy of a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartPolicy {
    No,
    Always,
    OnFailure,
    UnlessStopped,
    /// A value Compose does not know.
    Unknown(String),
}

impl RestartPolicy {
    /// Parses a Compose `restart` value (`on-failure:3` is `OnFailure`).
    pub fn parse(value: &str) -> Self {
        match value.split(':').next().unwrap_or_default() {
            "no" | "false" => Self::No,
            "always" => Self::Always,
            "on-failure" => Self::OnFailure,
            "unless-stopped" => Self::UnlessStopped,
            _ => Self::Unknown(value.to_owned()),
        }
    }
}

/// Logging configuration of a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logging {
    /// Driver name; `None` means the daemon default (`json-file`).
    pub driver: Option<String>,
    /// The `max-size` option, if any.
    pub max_size: Option<String>,
    pub location: Location,
}

/// Healthcheck of a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Healthcheck {
    /// `disable: true` or `test: ["NONE"]`.
    pub disabled: bool,
    pub interval: Option<Located<Duration>>,
    pub timeout: Option<Located<Duration>>,
    pub retries: Option<Located<u32>>,
    pub location: Location,
}

/// What a dependent service waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// Short syntax or `service_started`.
    Started,
    Healthy,
    CompletedSuccessfully,
}

/// An entry of `depends_on`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub service: String,
    pub condition: Condition,
    pub location: Location,
}

/// Transport protocol of a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
    Sctp,
}

/// An inclusive range of ports. A single port has `start == end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortRange {
    pub start: u16,
    pub end: u16,
}

impl PortRange {
    pub const fn single(port: u16) -> Self {
        Self {
            start: port,
            end: port,
        }
    }

    /// Whether two ranges share at least one port.
    pub const fn overlaps(self, other: Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

/// An entry of `ports`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMapping {
    /// Host IP the port is bound to; `None` means all interfaces.
    pub host_ip: Option<String>,
    /// Published host ports; `None` when the host port is chosen by Docker.
    pub published: Option<PortRange>,
    pub target: PortRange,
    pub protocol: Protocol,
    pub location: Location,
}

/// A host path used by a service, with whether it exists on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPath {
    pub path: String,
    pub kind: HostPathKind,
    pub exists: bool,
    pub location: Location,
}

/// Why a service uses a host path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPathKind {
    Bind,
    EnvFile,
    Secret,
    Config,
}

/// A volume or mount of a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mount {
    /// A named volume, which must be declared at the root.
    Named {
        name: String,
        target: String,
        location: Location,
    },
    /// A host directory or file mounted into the container.
    Bind { host_path: HostPath, target: String },
    /// An anonymous volume or a tmpfs: nothing to declare.
    Anonymous { target: String, location: Location },
    /// A volume entry Compose cannot parse.
    Malformed {
        raw: String,
        reason: String,
        location: Location,
    },
}

impl Mount {
    pub const fn location(&self) -> Location {
        match self {
            Self::Named { location, .. }
            | Self::Anonymous { location, .. }
            | Self::Malformed { location, .. } => *location,
            Self::Bind { host_path, .. } => host_path.location,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(raw: &str) -> (Option<String>, String, Option<String>, Option<String>) {
        let parsed = ImageRef::parse(raw, Location::default());
        (parsed.registry, parsed.name, parsed.tag, parsed.digest)
    }

    fn some(value: &str) -> Option<String> {
        Some(value.to_owned())
    }

    #[test]
    fn parses_a_bare_image_name() {
        assert_eq!(image("nginx"), (None, "nginx".into(), None, None));
    }

    #[test]
    fn parses_a_tag() {
        assert_eq!(
            image("nginx:1.27"),
            (None, "nginx".into(), some("1.27"), None)
        );
    }

    #[test]
    fn parses_a_registry_with_port_without_mistaking_it_for_a_tag() {
        assert_eq!(
            image("localhost:5000/team/api"),
            (some("localhost:5000"), "team/api".into(), None, None)
        );
    }

    #[test]
    fn parses_registry_tag_and_digest() {
        assert_eq!(
            image("ghcr.io/acme/api:2.0@sha256:abc"),
            (
                some("ghcr.io"),
                "acme/api".into(),
                some("2.0"),
                some("sha256:abc")
            )
        );
    }

    #[test]
    fn a_namespace_without_dot_is_not_a_registry() {
        assert_eq!(
            image("bitnami/redis"),
            (None, "bitnami/redis".into(), None, None)
        );
    }

    #[test]
    fn parses_restart_policies() {
        assert_eq!(RestartPolicy::parse("no"), RestartPolicy::No);
        assert_eq!(
            RestartPolicy::parse("on-failure:3"),
            RestartPolicy::OnFailure
        );
        assert_eq!(
            RestartPolicy::parse("unless-stopped"),
            RestartPolicy::UnlessStopped
        );
        assert_eq!(
            RestartPolicy::parse("sometimes"),
            RestartPolicy::Unknown("sometimes".into())
        );
    }

    #[test]
    fn port_ranges_overlap_when_they_share_a_port() {
        let range = PortRange {
            start: 8000,
            end: 8010,
        };
        assert!(range.overlaps(PortRange::single(8010)));
        assert!(!range.overlaps(PortRange::single(8011)));
    }
}
