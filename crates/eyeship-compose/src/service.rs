//! Mapping of one raw service to the domain model.

use eyeship_domain::{ImageRef, RestartPolicy, Service};

use crate::context::Context;
use crate::depends_on::map_depends_on;
use crate::files::map_files;
use crate::networks::map_service_networks;
use crate::ports::map_ports;
use crate::raw::{Key, RawRoot, RawService};
use crate::resources::{map_healthcheck, map_logging, map_memory_limit};
use crate::volumes::map_volumes;

/// Maps a service; its location is the location of its name. `roots` are
/// the roots of every file, where its secrets and configs are declared.
pub(crate) fn map_service(
    ctx: &Context<'_>,
    name: &Key,
    service: &RawService,
    roots: &[RawRoot],
) -> Service {
    Service {
        image: service.image.as_ref().map(|image| {
            let image = ctx.text(image);
            ImageRef::parse(&image.value, image.location)
        }),
        has_build: service.build.is_some(),
        container_name: service.container_name.as_ref().map(|n| ctx.text(n)),
        restart: service
            .restart
            .as_ref()
            .map(|restart| ctx.text(restart).map(|value| RestartPolicy::parse(&value))),
        memory_limit: map_memory_limit(ctx, service),
        logging: service.logging.as_ref().map(|l| map_logging(ctx, l)),
        healthcheck: service
            .healthcheck
            .as_ref()
            .map(|h| map_healthcheck(ctx, h)),
        depends_on: service
            .depends_on
            .as_ref()
            .map(|d| map_depends_on(ctx, d))
            .unwrap_or_default(),
        ports: service
            .ports
            .as_deref()
            .map(|p| map_ports(ctx, p))
            .unwrap_or_default(),
        networks: service
            .networks
            .as_ref()
            .map(|n| map_service_networks(ctx, n))
            .unwrap_or_default(),
        mounts: service
            .volumes
            .as_deref()
            .map(|v| map_volumes(ctx, v))
            .unwrap_or_default(),
        files: map_files(ctx, service, roots),
        ..Service::new(name.value.as_str(), ctx.location(&name.referenced))
    }
}

#[cfg(test)]
mod tests {
    use eyeship_domain::{Located, Mount};

    use super::*;
    use crate::context::test_support::*;

    fn first_service(yaml: &str) -> Service {
        let root: RawRoot = serde_saphyr::from_str(yaml).unwrap();
        let services = root.services.as_ref().unwrap();
        let (name, service) = &services.0[0];
        let empty = RawService::default();
        with_context(&[], &[("TAG", "1.27")], |ctx| {
            map_service(
                ctx,
                name,
                service.as_ref().unwrap_or(&empty),
                std::slice::from_ref(&root),
            )
        })
    }

    #[test]
    fn simple_fields() {
        let yaml = "services:
  web:
    image: ghcr.io/acme/web:${TAG}
    build: .
    container_name: web-1
    restart: on-failure:3
";
        let service = first_service(yaml);
        assert_eq!(service.name, "web");
        assert_eq!(service.location, at(2, 3));
        let image = service.image.unwrap();
        assert_eq!(
            (
                image.registry.as_deref(),
                image.name.as_str(),
                image.tag.as_deref()
            ),
            (Some("ghcr.io"), "acme/web", Some("1.27"))
        );
        assert_eq!(image.location, at(3, 12));
        assert!(service.has_build);
        assert_eq!(
            service.container_name,
            Some(Located::new("web-1".into(), at(5, 21)))
        );
        assert_eq!(
            service.restart,
            Some(Located::new(RestartPolicy::OnFailure, at(6, 14)))
        );
    }

    #[test]
    fn empty_service() {
        let service = first_service("services:\n  db:\n");
        assert_eq!(service, Service::new("db", at(2, 3)));
    }

    #[test]
    fn collections_are_mapped() {
        let yaml = "services:
  web:
    depends_on: [db]
    ports: [\"80:80\"]
    networks: [front]
    volumes: [\"data:/data\"]
    mem_limit: 1g
";
        let service = first_service(yaml);
        assert_eq!(service.depends_on.len(), 1);
        assert_eq!(service.ports.len(), 1);
        assert_eq!(service.networks.len(), 1);
        assert!(matches!(service.mounts[..], [Mount::Named { .. }]));
        assert_eq!(service.memory_limit.map(|m| m.value), Some(1 << 30));
    }
}
