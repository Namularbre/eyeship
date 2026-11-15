//! Merge of several Compose files (`-f a.yaml -f b.yaml`), following the
//! Compose merge rules (<https://docs.docker.com/reference/compose-file/merge/>).
//!
//! Design choice: each file is mapped to the domain model on its own, with
//! its own [`FileId`](eyeship_domain::FileId), then the mapped layers are
//! merged in order. Raw `serde_saphyr` positions carry no file, so merging
//! at the raw level would lose the file of origin; merging domain values
//! keeps, for free, the location of whichever file wrote the winning value.
//! Interpolation and unresolved variables thus stay per file.
//!
//! Rules applied to a service defined in several files, later file wins:
//! - scalars (`image`, `container_name`, `restart`, memory limit) are
//!   replaced; `build` in any file counts;
//! - `healthcheck` merges field by field; `disable`/`test` of the later
//!   file decide whether it is disabled only when that file writes them;
//! - `logging` merges field by field, but a different driver drops the
//!   earlier options;
//! - `ports` are appended without duplicates; `volumes` merge by container
//!   target; `depends_on` by service; `networks` by name; `env_file`,
//!   secrets and configs by host path;
//! - services keep the location of their first definition, and services
//!   new in a later file are appended.
//!
//! Root `networks` and `volumes` merge by name (external if any file says
//! so); the root `version` key is reported from the first file having it;
//! unresolved variables of every file are kept.

use eyeship_domain::{
    Declared, Healthcheck, Location, Logging, Mount, Project, Service, UnresolvedVar,
};

/// A service mapped from one file, with what the domain model cannot tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServiceLayer {
    pub service: Service,
    /// Whether its healthcheck writes `test` or `disable`, and so decides
    /// whether the merged healthcheck is disabled.
    pub sets_healthcheck_status: bool,
}

/// What one Compose file contributes to the project.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Layer {
    pub services: Vec<ServiceLayer>,
    pub networks: Vec<Declared>,
    pub volumes: Vec<Declared>,
    pub has_version_key: Option<Location>,
    pub unresolved_vars: Vec<UnresolvedVar>,
}

/// Merges the layers in order; `files` lists their files, indexed by `FileId`.
pub(crate) fn merge(files: Vec<String>, layers: Vec<Layer>) -> Project {
    let merged = layers.into_iter().reduce(merge_layers).unwrap_or_default();
    Project {
        files,
        services: merged
            .services
            .into_iter()
            .map(|layer| layer.service)
            .collect(),
        networks: merged.networks,
        volumes: merged.volumes,
        has_version_key: merged.has_version_key,
        unresolved_vars: merged.unresolved_vars,
    }
}

fn merge_layers(base: Layer, over: Layer) -> Layer {
    Layer {
        services: merge_with(
            base.services,
            over.services,
            |layer| Some(layer.service.name.clone()),
            merge_service,
        ),
        networks: merge_declared(base.networks, over.networks),
        volumes: merge_declared(base.volumes, over.volumes),
        has_version_key: base.has_version_key.or(over.has_version_key),
        unresolved_vars: base
            .unresolved_vars
            .into_iter()
            .chain(over.unresolved_vars)
            .collect(),
    }
}

fn merge_declared(base: Vec<Declared>, over: Vec<Declared>) -> Vec<Declared> {
    merge_with(
        base,
        over,
        |declared| Some(declared.name.clone()),
        |base, over| Declared {
            external: base.external || over.external,
            ..base
        },
    )
}

fn merge_service(base: ServiceLayer, over: ServiceLayer) -> ServiceLayer {
    let (base, over_status, over) = (base.service, over.sets_healthcheck_status, over.service);
    let service = Service {
        image: over.image.or(base.image),
        has_build: base.has_build || over.has_build,
        container_name: over.container_name.or(base.container_name),
        restart: over.restart.or(base.restart),
        memory_limit: over.memory_limit.or(base.memory_limit),
        logging: merge_logging(base.logging, over.logging),
        healthcheck: merge_healthcheck(base.healthcheck, over.healthcheck, over_status),
        depends_on: replace_by(base.depends_on, over.depends_on, |dependency| {
            Some(dependency.service.clone())
        }),
        ports: replace_by(base.ports, over.ports, |port| {
            Some((
                port.host_ip.clone(),
                port.published,
                port.target,
                port.protocol,
            ))
        }),
        networks: replace_by(base.networks, over.networks, |network| {
            Some(network.value.clone())
        }),
        mounts: replace_by(base.mounts, over.mounts, mount_target),
        files: replace_by(base.files, over.files, |file| {
            Some((file.kind, file.path.clone()))
        }),
        ..base
    };
    ServiceLayer {
        service,
        sets_healthcheck_status: false,
    }
}

fn mount_target(mount: &Mount) -> Option<String> {
    match mount {
        Mount::Named { target, .. }
        | Mount::Bind { target, .. }
        | Mount::Anonymous { target, .. } => Some(target.clone()),
        Mount::Malformed { .. } => None,
    }
}

fn merge_logging(base: Option<Logging>, over: Option<Logging>) -> Option<Logging> {
    match (base, over) {
        (Some(base), Some(over)) => {
            let same_driver = over.driver.is_none() || over.driver == base.driver;
            Some(Logging {
                max_size: if same_driver {
                    over.max_size.or(base.max_size)
                } else {
                    over.max_size
                },
                driver: over.driver.or(base.driver),
                location: over.location,
            })
        }
        (base, over) => over.or(base),
    }
}

fn merge_healthcheck(
    base: Option<Healthcheck>,
    over: Option<Healthcheck>,
    over_sets_status: bool,
) -> Option<Healthcheck> {
    match (base, over) {
        (Some(base), Some(over)) => Some(Healthcheck {
            disabled: if over_sets_status {
                over.disabled
            } else {
                base.disabled
            },
            interval: over.interval.or(base.interval),
            timeout: over.timeout.or(base.timeout),
            retries: over.retries.or(base.retries),
            location: over.location,
        }),
        (base, over) => over.or(base),
    }
}

/// Merges by key: a later item replaces the earlier one with the same key,
/// in its place; items without a known key are appended.
fn replace_by<T, K: PartialEq>(
    base: Vec<T>,
    over: Vec<T>,
    key: impl Fn(&T) -> Option<K>,
) -> Vec<T> {
    merge_with(base, over, key, |_, over| over)
}

/// Merges by key: an item whose key is already present is combined with the
/// earlier one, in its place; other items are appended in order.
fn merge_with<T, K: PartialEq>(
    base: Vec<T>,
    over: Vec<T>,
    key: impl Fn(&T) -> Option<K>,
    combine: impl Fn(T, T) -> T,
) -> Vec<T> {
    over.into_iter().fold(base, |mut merged, item| {
        let found = key(&item).and_then(|wanted| {
            merged
                .iter()
                .position(|known| key(known).as_ref() == Some(&wanted))
        });
        match found {
            Some(index) => {
                let earlier = merged.remove(index);
                merged.insert(index, combine(earlier, item));
            }
            None => merged.push(item),
        }
        merged
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use eyeship_domain::{
        Condition, Dependency, FileId, HostPath, HostPathKind, ImageRef, Located, PortMapping,
        PortRange, Protocol, RestartPolicy,
    };

    use super::*;

    const BASE: FileId = FileId(0);
    const OVER: FileId = FileId(1);

    fn at(file: FileId, line: u32) -> Location {
        Location::new(file, line, 1)
    }

    fn layer(file: FileId, services: Vec<Service>) -> Layer {
        Layer {
            services: services
                .into_iter()
                .map(|service| ServiceLayer {
                    service,
                    sets_healthcheck_status: false,
                })
                .collect(),
            networks: Vec::new(),
            volumes: Vec::new(),
            has_version_key: None,
            unresolved_vars: vec![UnresolvedVar {
                name: format!("V{}", file.0),
                service: None,
                location: at(file, 1),
            }],
        }
    }

    fn service(file: FileId, edit: impl FnOnce(&mut Service)) -> Service {
        let mut service = Service::new("web", at(file, 2));
        edit(&mut service);
        service
    }

    /// Merges one `web` service written in two files.
    fn merged(base: Service, over: Service) -> Service {
        merged_with_status(base, over, false)
    }

    fn merged_with_status(base: Service, over: Service, sets_status: bool) -> Service {
        let mut over_layer = layer(OVER, vec![over]);
        over_layer.services[0].sets_healthcheck_status = sets_status;
        let project = merge(
            vec!["a.yaml".into(), "b.yaml".into()],
            vec![layer(BASE, vec![base]), over_layer],
        );
        project.services.into_iter().next().unwrap()
    }

    fn port(published: u16, target: u16, file: FileId, line: u32) -> PortMapping {
        PortMapping {
            host_ip: None,
            published: Some(PortRange::single(published)),
            target: PortRange::single(target),
            protocol: Protocol::Tcp,
            location: at(file, line),
        }
    }

    fn named(name: &str, target: &str, file: FileId, line: u32) -> Mount {
        Mount::Named {
            name: name.into(),
            target: target.into(),
            location: at(file, line),
        }
    }

    fn dependency(name: &str, condition: Condition, file: FileId) -> Dependency {
        Dependency {
            service: name.into(),
            condition,
            location: at(file, 9),
        }
    }

    fn seconds(value: u64, file: FileId) -> Option<Located<Duration>> {
        Some(Located::new(Duration::from_secs(value), at(file, 7)))
    }

    #[test]
    fn a_single_layer_is_kept_as_is() {
        let web = service(BASE, |s| {
            s.ports = vec![port(80, 80, BASE, 3), port(80, 80, BASE, 4)]
        });
        let project = merge(vec!["a.yaml".into()], vec![layer(BASE, vec![web.clone()])]);
        assert_eq!(project.files, ["a.yaml"]);
        assert_eq!(project.services, [web]);
    }

    #[test]
    fn scalars_are_replaced_by_the_later_file_with_its_location() {
        let base = service(BASE, |s| {
            s.image = Some(ImageRef::parse("nginx:1", at(BASE, 3)));
            s.restart = Some(Located::new(RestartPolicy::No, at(BASE, 4)));
            s.memory_limit = Some(Located::new(1, at(BASE, 5)));
            s.has_build = true;
        });
        let over = service(OVER, |s| {
            s.image = Some(ImageRef::parse("nginx:2", at(OVER, 3)));
            s.container_name = Some(Located::new("web-1".into(), at(OVER, 4)));
        });
        let web = merged(base, over);
        assert_eq!(web.location, at(BASE, 2), "first definition");
        assert_eq!(web.image.unwrap().location, at(OVER, 3));
        assert_eq!(web.restart.unwrap().location, at(BASE, 4));
        assert_eq!(web.memory_limit.unwrap().value, 1);
        assert_eq!(web.container_name.unwrap().location, at(OVER, 4));
        assert!(web.has_build);
    }

    #[test]
    fn ports_are_appended_without_duplicates() {
        let base = service(BASE, |s| {
            s.ports = vec![port(80, 80, BASE, 3), port(443, 443, BASE, 4)]
        });
        let over = service(OVER, |s| {
            s.ports = vec![port(443, 443, OVER, 3), port(8080, 80, OVER, 4)]
        });
        let locations: Vec<_> = merged(base, over)
            .ports
            .iter()
            .map(|p| p.location)
            .collect();
        assert_eq!(locations, [at(BASE, 3), at(OVER, 3), at(OVER, 4)]);
    }

    #[test]
    fn volumes_merge_by_container_target() {
        let base = service(BASE, |s| {
            s.mounts = vec![
                named("data", "/data", BASE, 3),
                named("logs", "/logs", BASE, 4),
            ];
        });
        let over = service(OVER, |s| {
            s.mounts = vec![
                named("fast", "/data", OVER, 3),
                named("tmp", "/tmp", OVER, 4),
            ];
        });
        assert_eq!(
            merged(base, over).mounts,
            [
                named("fast", "/data", OVER, 3),
                named("logs", "/logs", BASE, 4),
                named("tmp", "/tmp", OVER, 4),
            ]
        );
    }

    #[test]
    fn depends_on_merges_by_service() {
        let base = service(BASE, |s| {
            s.depends_on = vec![
                dependency("db", Condition::Started, BASE),
                dependency("cache", Condition::Started, BASE),
            ];
        });
        let over = service(OVER, |s| {
            s.depends_on = vec![dependency("db", Condition::Healthy, OVER)]
        });
        assert_eq!(
            merged(base, over).depends_on,
            [
                dependency("db", Condition::Healthy, OVER),
                dependency("cache", Condition::Started, BASE),
            ]
        );
    }

    #[test]
    fn networks_merge_by_name_and_host_files_by_path() {
        let file = |path: &str, kind, f| HostPath {
            path: path.into(),
            kind,
            exists: true,
            location: at(f, 5),
        };
        let base = service(BASE, |s| {
            s.networks = vec![Located::new("back".into(), at(BASE, 3))];
            s.files = vec![file("a.env", HostPathKind::EnvFile, BASE)];
        });
        let over = service(OVER, |s| {
            s.networks = vec![
                Located::new("back".into(), at(OVER, 3)),
                Located::new("front".into(), at(OVER, 4)),
            ];
            s.files = vec![
                file("a.env", HostPathKind::EnvFile, OVER),
                file("b.env", HostPathKind::EnvFile, OVER),
            ];
        });
        let web = merged(base, over);
        let networks: Vec<_> = web.networks.iter().map(|n| n.value.as_str()).collect();
        assert_eq!(networks, ["back", "front"]);
        let files: Vec<_> = web.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(files, ["a.env", "b.env"]);
    }

    #[test]
    fn healthcheck_merges_field_by_field() {
        let healthcheck = |file, disabled, interval, timeout| Healthcheck {
            disabled,
            interval,
            timeout,
            retries: None,
            location: at(file, 6),
        };
        let base = service(BASE, |s| {
            s.healthcheck = Some(healthcheck(BASE, true, seconds(30, BASE), seconds(5, BASE)));
        });
        let over = service(OVER, |s| {
            s.healthcheck = Some(healthcheck(OVER, false, None, seconds(10, OVER)));
        });
        let result = merged(base.clone(), over.clone()).healthcheck.unwrap();
        assert_eq!(result.interval, seconds(30, BASE));
        assert_eq!(result.timeout, seconds(10, OVER));
        assert_eq!(result.location, at(OVER, 6));
        assert!(
            result.disabled,
            "the later file does not write test or disable"
        );
        let result = merged_with_status(base, over, true).healthcheck.unwrap();
        assert!(!result.disabled, "the later file writes a test");
    }

    #[test]
    fn logging_options_are_dropped_when_the_driver_changes() {
        let logging = |file, driver: Option<&str>, max_size: Option<&str>| Logging {
            driver: driver.map(str::to_owned),
            max_size: max_size.map(str::to_owned),
            location: at(file, 8),
        };
        let with = |file, l| service(file, |s| s.logging = Some(l));
        let base = || with(BASE, logging(BASE, Some("json-file"), Some("10m")));
        assert_eq!(
            merged(base(), with(OVER, logging(OVER, None, None))).logging,
            Some(logging(OVER, Some("json-file"), Some("10m")))
        );
        assert_eq!(
            merged(base(), with(OVER, logging(OVER, Some("local"), None))).logging,
            Some(logging(OVER, Some("local"), None))
        );
    }

    #[test]
    fn services_and_root_resources_merge_by_name() {
        let declared = |name: &str, external, file| Declared {
            name: name.into(),
            external,
            location: at(file, 10),
        };
        let mut base = layer(BASE, vec![Service::new("web", at(BASE, 2))]);
        base.networks = vec![declared("back", true, BASE)];
        base.has_version_key = Some(at(BASE, 1));
        let mut over = layer(
            OVER,
            vec![
                Service::new("db", at(OVER, 2)),
                Service::new("web", at(OVER, 3)),
            ],
        );
        over.networks = vec![
            declared("back", false, OVER),
            declared("front", false, OVER),
        ];
        over.volumes = vec![declared("data", false, OVER)];
        over.has_version_key = Some(at(OVER, 1));
        let project = merge(vec!["a.yaml".into(), "b.yaml".into()], vec![base, over]);
        let services: Vec<_> = project
            .services
            .iter()
            .map(|s| (s.name.as_str(), s.location))
            .collect();
        assert_eq!(services, [("web", at(BASE, 2)), ("db", at(OVER, 2))]);
        assert_eq!(
            project.networks,
            [declared("back", true, BASE), declared("front", false, OVER)]
        );
        assert_eq!(project.volumes, [declared("data", false, OVER)]);
        assert_eq!(project.has_version_key, Some(at(BASE, 1)));
        let vars: Vec<_> = project
            .unresolved_vars
            .iter()
            .map(|v| v.name.as_str())
            .collect();
        assert_eq!(vars, ["V0", "V1"]);
    }
}
