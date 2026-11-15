//! Entry point: reads a Compose file and builds the domain project.

use std::path::{Path, PathBuf};

use eyeship_domain::{FileId, Project};
use serde::Deserialize;
use serde_saphyr::Spanned;

use crate::context::Context;
use crate::discovery::discover;
use crate::dotenv::parse_dotenv;
use crate::env::Env;
use crate::error::LoadError;
use crate::fs::FileSystem;
use crate::networks::map_declared;
use crate::raw::{Node, RawRoot, RawService};
use crate::service::map_service;
use crate::unresolved::{unresolved_vars, version_key};
use crate::vars::VarSource;

/// What to load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadRequest {
    /// The Compose file; `None` discovers the default file in `working_dir`.
    pub file: Option<PathBuf>,
    /// An explicit env file replacing the `.env` next to the Compose file.
    pub env_file: Option<PathBuf>,
    /// Directory against which relative request paths resolve.
    pub working_dir: PathBuf,
}

/// Loads a Compose project: reads, interpolates and normalizes it, and
/// checks the host paths it uses.
pub fn load(
    request: &LoadRequest,
    fs: &dyn FileSystem,
    env: &dyn Env,
) -> Result<Project, LoadError> {
    let (path, shown) = locate(request, fs)?;
    let content = read(fs, &path, &shown)?;
    let dir = path
        .parent()
        .map_or_else(|| request.working_dir.clone(), Path::to_path_buf);
    let dotenv = read_dotenv(request, &dir, fs)?;
    let vars = VarSource::new(env, dotenv);
    let ctx = Context {
        file: FileId(0),
        dir,
        fs,
        vars: &vars,
    };
    let raw: RawRoot = parse(&content, &shown)?;
    let tree: Spanned<Node> = parse(&content, &shown)?;
    Ok(build_project(
        &ctx,
        shown.display().to_string(),
        &raw,
        &tree,
    ))
}

/// The file to read and the path to show the user.
fn locate(request: &LoadRequest, fs: &dyn FileSystem) -> Result<(PathBuf, PathBuf), LoadError> {
    match &request.file {
        Some(file) => Ok((request.working_dir.join(file), file.clone())),
        None => {
            let found =
                discover(&request.working_dir, fs).ok_or_else(|| LoadError::NoComposeFile {
                    dir: request.working_dir.clone(),
                })?;
            let shown = found
                .strip_prefix(&request.working_dir)
                .map_or_else(|_| found.clone(), Path::to_path_buf);
            Ok((found, shown))
        }
    }
}

fn read(fs: &dyn FileSystem, path: &Path, shown: &Path) -> Result<String, LoadError> {
    fs.read_to_string(path)
        .map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => LoadError::NotFound {
                path: shown.to_path_buf(),
            },
            _ => LoadError::Io {
                path: shown.to_path_buf(),
                source,
            },
        })
}

/// The explicit env file (which must exist), else the optional `.env`.
fn read_dotenv(
    request: &LoadRequest,
    dir: &Path,
    fs: &dyn FileSystem,
) -> Result<Vec<(String, String)>, LoadError> {
    match &request.env_file {
        Some(env_file) => read(fs, &request.working_dir.join(env_file), env_file),
        None => {
            let default = dir.join(".env");
            if fs.exists(&default) {
                read(fs, &default, &default)
            } else {
                Ok(String::new())
            }
        }
    }
    .map(|content| parse_dotenv(&content))
}

fn parse<'de, T: Deserialize<'de>>(content: &'de str, shown: &Path) -> Result<T, LoadError> {
    serde_saphyr::from_str(content).map_err(|error| {
        let (line, column) = error.location().map_or((1, 1), |location| {
            (
                u32::try_from(location.line()).unwrap_or(u32::MAX),
                u32::try_from(location.column()).unwrap_or(u32::MAX),
            )
        });
        let full = error.without_snippet().to_string();
        let suffix = format!(" at line {line}, column {column}");
        let message = full.strip_suffix(&suffix).unwrap_or(&full).to_owned();
        LoadError::InvalidYaml {
            path: shown.to_path_buf(),
            message,
            line,
            column,
        }
    })
}

fn build_project(ctx: &Context<'_>, shown: String, raw: &RawRoot, tree: &Spanned<Node>) -> Project {
    let empty = RawService::default();
    Project {
        files: vec![shown],
        services: raw
            .services
            .iter()
            .flat_map(|services| services.0.iter())
            .map(|(name, service)| map_service(ctx, name, service.as_ref().unwrap_or(&empty), raw))
            .collect(),
        networks: raw
            .networks
            .as_ref()
            .map(|networks| map_declared(ctx, networks))
            .unwrap_or_default(),
        volumes: raw
            .volumes
            .as_ref()
            .map(|volumes| map_declared(ctx, volumes))
            .unwrap_or_default(),
        has_version_key: version_key(ctx, tree),
        unresolved_vars: unresolved_vars(ctx, tree),
    }
}

#[cfg(test)]
mod tests {
    use eyeship_domain::{Condition, Location, Mount, UnresolvedVar};

    use super::*;
    use crate::env::MemoryEnv;
    use crate::fs::MemoryFileSystem;

    fn request(file: Option<&str>) -> LoadRequest {
        LoadRequest {
            file: file.map(PathBuf::from),
            env_file: None,
            working_dir: PathBuf::from("/work"),
        }
    }

    fn load_files(
        files: &[(&str, &str)],
        request: &LoadRequest,
        env: &[(&str, &str)],
    ) -> Result<Project, LoadError> {
        let fs = MemoryFileSystem::new(files.iter().copied());
        load(request, &fs, &MemoryEnv::new(env.iter().copied()))
    }

    fn at(line: u32, column: u32) -> Location {
        Location::new(FileId(0), line, column)
    }

    fn image_tag(project: &Project, service: &str) -> Option<String> {
        project.service(service)?.image.as_ref()?.tag.clone()
    }

    #[test]
    fn discovers_the_default_file_and_shows_it_relative() {
        let files = [(
            "/work/docker-compose.yml",
            "services:\n  web:\n    image: nginx\n",
        )];
        let project = load_files(&files, &request(None), &[]).unwrap();
        assert_eq!(project.files, ["docker-compose.yml"]);
        assert_eq!(project.services[0].name, "web");
    }

    #[test]
    fn shows_a_given_file_as_written() {
        let files = [("/work/deploy/prod.yaml", "services: {}\n")];
        let project = load_files(&files, &request(Some("deploy/prod.yaml")), &[]).unwrap();
        assert_eq!(project.files, ["deploy/prod.yaml"]);
        assert!(project.services.is_empty());
    }

    #[test]
    fn missing_file_and_missing_discovery_are_errors() {
        let error = load_files(&[], &request(Some("nope.yaml")), &[]).unwrap_err();
        assert!(matches!(error, LoadError::NotFound { path } if path == Path::new("nope.yaml")));
        let error = load_files(&[("/work/README.md", "")], &request(None), &[]).unwrap_err();
        assert!(matches!(error, LoadError::NoComposeFile { dir } if dir == Path::new("/work")));
    }

    #[test]
    fn invalid_yaml_reports_its_position() {
        let files = [(
            "/work/compose.yaml",
            "services:\n  web:\n    image: [nginx\n",
        )];
        let error = load_files(&files, &request(None), &[]).unwrap_err();
        let LoadError::InvalidYaml {
            path,
            line,
            column,
            message,
        } = &error
        else {
            panic!("expected invalid YAML, got {error}")
        };
        assert_eq!(path, Path::new("compose.yaml"));
        assert!(*line >= 3, "line {line}, column {column}");
        assert!(
            !message.is_empty() && !message.contains(" at line "),
            "{message}"
        );
    }

    #[test]
    fn wrong_structure_is_an_invalid_yaml_error_with_position() {
        let files = [("/work/compose.yaml", "services:\n  web: nginx\n")];
        let error = load_files(&files, &request(None), &[]).unwrap_err();
        let LoadError::InvalidYaml { line, message, .. } = &error else {
            panic!("expected invalid YAML, got {error}")
        };
        assert_eq!(*line, 2);
        assert!(message.contains("mapping"), "{message}");
    }

    #[test]
    fn a_document_that_is_not_a_mapping_is_rejected() {
        let files = [("/work/compose.yaml", "just text\n")];
        let error = load_files(&files, &request(None), &[]).unwrap_err();
        assert!(
            matches!(error, LoadError::InvalidYaml { line: 1, .. }),
            "{error}"
        );
    }

    #[test]
    fn variables_come_from_env_then_dotenv_next_to_the_compose_file() {
        let files = [
            (
                "/work/app/compose.yaml",
                "services:\n  a:\n    image: a:${A}\n  b:\n    image: b:${B}\n",
            ),
            ("/work/app/.env", "A=from-file\nB=from-file\n"),
        ];
        let project = load_files(
            &files,
            &request(Some("app/compose.yaml")),
            &[("A", "from-env")],
        )
        .unwrap();
        assert_eq!(image_tag(&project, "a").as_deref(), Some("from-env"));
        assert_eq!(image_tag(&project, "b").as_deref(), Some("from-file"));
        assert!(project.unresolved_vars.is_empty());
    }

    #[test]
    fn an_explicit_env_file_replaces_dotenv_and_must_exist() {
        let files = [
            ("/work/compose.yaml", "services:\n  a:\n    image: a:${A}\n"),
            ("/work/.env", "A=dotenv\n"),
            ("/work/prod.env", "A=prod\n"),
        ];
        let with_env_file = |path: &str| LoadRequest {
            env_file: Some(PathBuf::from(path)),
            ..request(None)
        };
        let project = load_files(&files, &with_env_file("prod.env"), &[]).unwrap();
        assert_eq!(image_tag(&project, "a").as_deref(), Some("prod"));
        let error = load_files(&files, &with_env_file("missing.env"), &[]).unwrap_err();
        assert!(matches!(error, LoadError::NotFound { path } if path == Path::new("missing.env")));
    }

    #[test]
    fn unresolved_variables_are_reported_with_service_and_location() {
        let files = [(
            "/work/compose.yaml",
            "services:\n  a:\n    image: a:${TAG}\n    environment:\n      DB: ${DB_URL:?required}\n",
        )];
        let project = load_files(&files, &request(None), &[]).unwrap();
        let var = |name: &str, line, column| UnresolvedVar {
            name: name.into(),
            service: Some("a".into()),
            location: at(line, column),
        };
        assert_eq!(
            project.unresolved_vars,
            [var("TAG", 3, 12), var("DB_URL", 5, 11)]
        );
    }

    #[test]
    fn full_project_with_anchors_and_merge_keys() {
        let yaml = "version: \"3.8\"
x-defaults: &defaults
  restart: unless-stopped
  logging:
    driver: json-file
services:
  api:
    <<: *defaults
    image: acme/api:1.0
    depends_on:
      db:
        condition: service_healthy
    volumes:
      - data:/data
      - ./config:/config:ro
    networks: [back]
  db:
    <<: *defaults
    image: postgres:16
    healthcheck:
      test: [CMD, pg_isready]
networks:
  back:
volumes:
  data:
    external: true
";
        let files = [("/work/compose.yaml", yaml), ("/work/config/app.toml", "")];
        let project = load_files(&files, &request(None), &[]).unwrap();
        assert_eq!(project.has_version_key, Some(at(1, 1)));
        let names: Vec<_> = project.services.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["api", "db"]);
        let api = project.service("api").unwrap();
        assert_eq!(api.location, at(7, 3));
        // Merged values point at the alias inside the service that uses them.
        assert_eq!(api.restart.as_ref().unwrap().location, at(8, 9));
        assert_eq!(
            project
                .service("db")
                .unwrap()
                .restart
                .as_ref()
                .unwrap()
                .location,
            at(18, 9)
        );
        assert_eq!(
            api.logging.as_ref().unwrap().driver.as_deref(),
            Some("json-file")
        );
        assert_eq!(api.image.as_ref().unwrap().location, at(9, 12));
        assert_eq!(api.depends_on[0].condition, Condition::Healthy);
        assert_eq!(api.depends_on[0].location, at(11, 7));
        assert!(matches!(&api.mounts[1], Mount::Bind { host_path, .. } if host_path.exists));
        assert_eq!(api.mounts[1].location(), at(15, 9));
        assert_eq!(project.networks[0].name, "back");
        assert!(project.volumes[0].external);
        assert!(project.service("db").unwrap().healthcheck.is_some());
    }
}
