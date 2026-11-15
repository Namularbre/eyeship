//! Raw serde model of a Compose file, close to the YAML.
//!
//! Every value the mapping needs is wrapped in `Spanned` to keep its
//! location. Unknown keys and `x-*` extensions are ignored. Shapes that
//! accept several forms are untagged enums only when their inner positions
//! are not needed; the enum is then wrapped in `Spanned` as a whole.

mod collections;
mod node;
mod scalar;

use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_saphyr::Spanned;

pub(crate) use collections::{Entries, Key, ListOrMap, OneOrMany};
pub(crate) use node::Node;
pub(crate) use scalar::Scalar;

/// A located scalar.
pub(crate) type Text = Spanned<Scalar>;

/// The root of a Compose file.
#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a Compose file (a mapping with `services`)")]
pub(crate) struct RawRoot {
    pub services: Option<Entries<Option<RawService>>>,
    pub networks: Option<Entries<Option<RawDeclared>>>,
    pub volumes: Option<Entries<Option<RawDeclared>>>,
    pub secrets: Option<Entries<Option<RawFileSource>>>,
    pub configs: Option<Entries<Option<RawFileSource>>>,
}

/// A network or volume declared at the root.
#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a network or volume definition")]
pub(crate) struct RawDeclared {
    pub external: Option<RawExternal>,
}

/// `external: true`, or the legacy `external: {name: ...}`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum RawExternal {
    Flag(Scalar),
    Legacy(IgnoredAny),
}

/// A secret or config declared at the root.
#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a secret or config definition")]
pub(crate) struct RawFileSource {
    pub file: Option<Text>,
}

/// One service.
#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a service definition (a mapping)")]
pub(crate) struct RawService {
    pub image: Option<Text>,
    pub build: Option<IgnoredAny>,
    pub container_name: Option<Text>,
    pub restart: Option<Text>,
    pub mem_limit: Option<Text>,
    pub deploy: Option<RawDeploy>,
    pub logging: Option<Spanned<RawLogging>>,
    pub healthcheck: Option<Spanned<RawHealthcheck>>,
    pub depends_on: Option<ListOrMap<Option<RawDependsOn>>>,
    pub ports: Option<Vec<Spanned<RawPort>>>,
    pub volumes: Option<Vec<Spanned<RawVolume>>>,
    pub networks: Option<ListOrMap<IgnoredAny>>,
    pub env_file: Option<Spanned<OneOrMany<RawEnvFile>>>,
    pub secrets: Option<Vec<Spanned<RawFileRef>>>,
    pub configs: Option<Vec<Spanned<RawFileRef>>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a `deploy` mapping")]
pub(crate) struct RawDeploy {
    pub resources: Option<RawResources>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a `resources` mapping")]
pub(crate) struct RawResources {
    pub limits: Option<RawLimits>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a `limits` mapping")]
pub(crate) struct RawLimits {
    pub memory: Option<Text>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a `logging` mapping")]
pub(crate) struct RawLogging {
    pub driver: Option<Text>,
    pub options: Option<RawLogOptions>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a mapping of logging options")]
pub(crate) struct RawLogOptions {
    #[serde(rename = "max-size")]
    pub max_size: Option<Text>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a `healthcheck` mapping")]
pub(crate) struct RawHealthcheck {
    pub test: Option<OneOrMany<Scalar>>,
    pub disable: Option<Scalar>,
    pub interval: Option<Text>,
    pub timeout: Option<Text>,
    pub retries: Option<Text>,
}

/// The long form of a `depends_on` entry.
#[derive(Debug, Default, Deserialize)]
#[serde(expecting = "a dependency definition")]
pub(crate) struct RawDependsOn {
    pub condition: Option<Text>,
}

/// An entry of `ports`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum RawPort {
    Short(Scalar),
    Long {
        target: Option<Scalar>,
        published: Option<Scalar>,
        host_ip: Option<Scalar>,
        protocol: Option<Scalar>,
    },
    Other(IgnoredAny),
}

/// An entry of a service's `volumes`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum RawVolume {
    Short(Scalar),
    Long {
        #[serde(rename = "type")]
        kind: Option<Scalar>,
        source: Option<Scalar>,
        target: Option<Scalar>,
    },
    Other(IgnoredAny),
}

/// An entry of `env_file`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum RawEnvFile {
    Path(Scalar),
    Long {
        path: Option<Scalar>,
        required: Option<Scalar>,
    },
    Other(IgnoredAny),
}

/// An entry of a service's `secrets` or `configs`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum RawFileRef {
    Short(Scalar),
    Long { source: Option<Scalar> },
    Other(IgnoredAny),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_services_in_file_order() {
        let root: RawRoot =
            serde_saphyr::from_str("x-a: 1\nservices:\n  web:\n    image: nginx\n  db:\nnope: 2\n")
                .unwrap();
        let names: Vec<_> = root
            .services
            .unwrap_or_default()
            .0
            .into_iter()
            .map(|(key, service)| (key.value.0, service.is_some()))
            .collect();
        assert_eq!(names, [("web".to_owned(), true), ("db".to_owned(), false)]);
    }

    #[test]
    fn unparsable_list_entries_do_not_fail_the_file() {
        let service: RawService =
            serde_saphyr::from_str("ports: [[1], 80]\nvolumes: [[x]]\n").unwrap();
        let ports = service.ports.unwrap_or_default();
        assert!(matches!(ports[0].value, RawPort::Other(_)));
        assert!(matches!(ports[1].value, RawPort::Short(_)));
    }
}
