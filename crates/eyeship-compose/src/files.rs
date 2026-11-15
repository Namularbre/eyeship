//! Host files used by a service: `env_file`, file secrets and file configs.

use eyeship_domain::{HostPath, HostPathKind, Location};
use serde_saphyr::Spanned;

use crate::context::Context;
use crate::raw::{Entries, OneOrMany, RawEnvFile, RawFileRef, RawFileSource, RawRoot, RawService};

/// Every host file a service needs, in the order `env_file`, secrets, configs.
pub(crate) fn map_files(ctx: &Context<'_>, service: &RawService, root: &RawRoot) -> Vec<HostPath> {
    let env_files = service
        .env_file
        .as_ref()
        .map(|env_file| map_env_files(ctx, env_file))
        .unwrap_or_default();
    let secrets = map_references(
        ctx,
        service.secrets.as_deref(),
        root.secrets.as_ref(),
        HostPathKind::Secret,
    );
    let configs = map_references(
        ctx,
        service.configs.as_deref(),
        root.configs.as_ref(),
        HostPathKind::Config,
    );
    env_files
        .into_iter()
        .chain(secrets)
        .chain(configs)
        .collect()
}

fn map_env_files(ctx: &Context<'_>, env_file: &Spanned<OneOrMany<RawEnvFile>>) -> Vec<HostPath> {
    match &env_file.value {
        OneOrMany::One(path) => vec![env_path(
            ctx,
            path.as_str(),
            ctx.location(&env_file.referenced),
        )],
        OneOrMany::Many(entries) => entries
            .iter()
            .filter_map(|entry| {
                let location = ctx.location(&entry.referenced);
                match &entry.value {
                    RawEnvFile::Path(path) => Some(env_path(ctx, path.as_str(), location)),
                    RawEnvFile::Long { path, required } => {
                        let optional = required
                            .as_ref()
                            .is_some_and(|r| ctx.interpolate(r.as_str()) == "false");
                        (!optional)
                            .then_some(path.as_ref())
                            .flatten()
                            .map(|path| env_path(ctx, path.as_str(), location))
                    }
                    RawEnvFile::Other(_) => None,
                }
            })
            .collect(),
    }
}

fn env_path(ctx: &Context<'_>, raw: &str, location: Location) -> HostPath {
    ctx.host_path(ctx.interpolate(raw), HostPathKind::EnvFile, location)
}

/// Resolves a service's `secrets` or `configs` against the root declarations;
/// only declarations with a `file:` produce a host path.
fn map_references(
    ctx: &Context<'_>,
    references: Option<&[Spanned<RawFileRef>]>,
    declared: Option<&Entries<Option<RawFileSource>>>,
    kind: HostPathKind,
) -> Vec<HostPath> {
    references
        .unwrap_or_default()
        .iter()
        .filter_map(|reference| {
            let name = match &reference.value {
                RawFileRef::Short(name) | RawFileRef::Long { source: Some(name) } => {
                    ctx.interpolate(name.as_str())
                }
                RawFileRef::Long { source: None } | RawFileRef::Other(_) => return None,
            };
            let file = declared?
                .0
                .iter()
                .find(|(key, _)| key.value.as_str() == name)
                .and_then(|(_, source)| source.as_ref()?.file.as_ref())?;
            let path = ctx.interpolate(file.value.as_str());
            (!path.is_empty())
                .then(|| ctx.host_path(path, kind, ctx.location(&reference.referenced)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::test_support::*;

    const PRESENT: &str = "/app/present.env";

    fn files(root_yaml: &str) -> Vec<(String, HostPathKind, bool, u32)> {
        let root: RawRoot = serde_saphyr::from_str(root_yaml).unwrap();
        let services = root.services.as_ref().unwrap();
        let service = services.0[0].1.as_ref().unwrap();
        with_context(
            &[PRESENT, "/app/secrets/db"],
            &[("NAME", "present")],
            |ctx| map_files(ctx, service, &root),
        )
        .into_iter()
        .map(|p| (p.path, p.kind, p.exists, p.location.line))
        .collect()
    }

    fn file(
        path: &str,
        kind: HostPathKind,
        exists: bool,
        line: u32,
    ) -> (String, HostPathKind, bool, u32) {
        (path.to_owned(), kind, exists, line)
    }

    #[test]
    fn env_file_as_a_string() {
        assert_eq!(
            files("services:\n  a:\n    env_file: ${NAME}.env\n"),
            [file("present.env", HostPathKind::EnvFile, true, 3)]
        );
    }

    #[test]
    fn env_file_as_a_list_skips_optional_files() {
        let yaml = "services:
  a:
    env_file:
      - missing.env
      - path: ./present.env
      - path: optional.env
        required: false
      - path: /etc/abs.env
        required: true
";
        assert_eq!(
            files(yaml),
            [
                file("missing.env", HostPathKind::EnvFile, false, 4),
                file("./present.env", HostPathKind::EnvFile, true, 5),
                file("/etc/abs.env", HostPathKind::EnvFile, true, 8),
            ]
        );
    }

    #[test]
    fn file_secrets_and_configs() {
        let yaml = "services:
  a:
    secrets:
      - db
      - source: api
      - external_one
    configs: [nginx]
secrets:
  db:
    file: ./secrets/db
  api:
    file: ./secrets/api
  external_one:
    external: true
configs:
  nginx:
    file: /etc/nginx.conf
";
        assert_eq!(
            files(yaml),
            [
                file("./secrets/db", HostPathKind::Secret, true, 4),
                file("./secrets/api", HostPathKind::Secret, false, 5),
                file("/etc/nginx.conf", HostPathKind::Config, true, 7),
            ]
        );
    }

    #[test]
    fn undeclared_references_are_ignored() {
        assert_eq!(files("services:\n  a:\n    secrets: [nope]\n"), []);
    }
}
