//! Entry point: reads one or several Compose files and builds the domain
//! project. Several files are mapped one by one, then merged (see
//! [`crate::merge`]).

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
use crate::merge::{Layer, ServiceLayer, merge};
use crate::networks::map_declared;
use crate::raw::{Node, RawRoot, RawService};
use crate::resources::healthcheck_sets_status;
use crate::service::map_service;
use crate::unresolved::{unresolved_vars, version_key};
use crate::vars::VarSource;

/// What to load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadRequest {
    /// The Compose files, merged in order; empty discovers the default file
    /// in `working_dir`.
    pub files: Vec<PathBuf>,
    /// An explicit env file replacing the `.env` next to the first Compose file.
    pub env_file: Option<PathBuf>,
    /// Directory against which relative request paths resolve.
    pub working_dir: PathBuf,
}

/// One Compose file read and parsed.
struct Parsed {
    shown: String,
    raw: RawRoot,
    tree: Spanned<Node>,
}

/// Loads a Compose project: reads, interpolates, normalizes and merges its
/// files, and checks the host paths they use.
///
/// As in Compose, the directory of the first file is the project directory:
/// relative paths of every file resolve against it, and the `.env` is read
/// from it.
pub fn load(
    request: &LoadRequest,
    fs: &dyn FileSystem,
    env: &dyn Env,
) -> Result<Project, LoadError> {
    let sources = locate(request, fs)?;
    let dir = sources
        .first()
        .and_then(|(path, _)| path.parent())
        .map_or_else(|| request.working_dir.clone(), Path::to_path_buf);
    let dotenv = read_dotenv(request, &dir, fs)?;
    let vars = VarSource::new(env, dotenv);
    let parsed = sources
        .iter()
        .map(|(path, shown)| {
            let content = read(fs, path, shown)?;
            Ok(Parsed {
                shown: shown.display().to_string(),
                raw: parse(&content, shown)?,
                tree: parse(&content, shown)?,
            })
        })
        .collect::<Result<Vec<_>, LoadError>>()?;
    let files: Vec<String> = parsed.iter().map(|p| p.shown.clone()).collect();
    let (roots, trees): (Vec<RawRoot>, Vec<Spanned<Node>>) = parsed
        .into_iter()
        .map(|parsed| (parsed.raw, parsed.tree))
        .unzip();
    let layers = roots
        .iter()
        .zip(&trees)
        .enumerate()
        .map(|(index, (raw, tree))| {
            let ctx = Context {
                file: FileId(u32::try_from(index).unwrap_or(u32::MAX)),
                dir: dir.clone(),
                fs,
                vars: &vars,
            };
            build_layer(&ctx, raw, tree, &roots)
        })
        .collect();
    Ok(merge(files, layers))
}

/// The files to read, each with the path to show the user.
fn locate(
    request: &LoadRequest,
    fs: &dyn FileSystem,
) -> Result<Vec<(PathBuf, PathBuf)>, LoadError> {
    if request.files.is_empty() {
        let found = discover(&request.working_dir, fs).ok_or_else(|| LoadError::NoComposeFile {
            dir: request.working_dir.clone(),
        })?;
        let shown = found
            .strip_prefix(&request.working_dir)
            .map_or_else(|_| found.clone(), Path::to_path_buf);
        Ok(vec![(found, shown)])
    } else {
        Ok(request
            .files
            .iter()
            .map(|file| locate_one(&request.working_dir.join(file), file, fs))
            .collect())
    }
}

/// A given path, or the default Compose file inside it when it is a
/// directory (`eyeship compose ./api`).
fn locate_one(path: &Path, shown: &Path, fs: &dyn FileSystem) -> (PathBuf, PathBuf) {
    discover(path, fs).map_or_else(
        || (path.to_path_buf(), shown.to_path_buf()),
        |found| {
            let name = found.file_name().map(PathBuf::from).unwrap_or_default();
            (found, shown.join(name))
        },
    )
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

/// Default parser budget, minus the alias/anchor ratio heuristic: Compose
/// files legitimately reuse a few `x-` anchors hundreds of times. The absolute
/// caps (aliases, nodes, depth) still guard against alias bombs.
fn yaml_options() -> serde_saphyr::Options {
    serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            enforce_alias_anchor_ratio: false,
        },
    }
}

fn parse<'de, T: Deserialize<'de>>(content: &'de str, shown: &Path) -> Result<T, LoadError> {
    serde_saphyr::from_str_with_options(content, yaml_options()).map_err(|error| {
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

/// Maps one file; `roots` are the roots of every file, for cross-file
/// declarations.
fn build_layer(ctx: &Context<'_>, raw: &RawRoot, tree: &Spanned<Node>, roots: &[RawRoot]) -> Layer {
    let empty = RawService::default();
    Layer {
        services: raw
            .services
            .iter()
            .flat_map(|services| services.0.iter())
            .map(|(name, service)| {
                let service = service.as_ref().unwrap_or(&empty);
                ServiceLayer {
                    service: map_service(ctx, name, service, roots),
                    sets_healthcheck_status: healthcheck_sets_status(service),
                }
            })
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
            files: file.map(PathBuf::from).into_iter().collect(),
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
    fn a_given_directory_discovers_its_default_file() {
        let files = [(
            "/work/api/compose.yml",
            "services:\n  api:\n    image: nginx\n",
        )];
        let project = load_files(&files, &request(Some("api")), &[]).unwrap();
        let expected = Path::new("api").join("compose.yml").display().to_string();
        assert_eq!(project.files, [expected]);
        assert_eq!(project.services[0].name, "api");
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

    fn files_request(files: &[&str]) -> LoadRequest {
        LoadRequest {
            files: files.iter().map(PathBuf::from).collect(),
            ..request(None)
        }
    }

    #[test]
    fn several_files_are_merged_with_the_file_of_each_value() {
        let files = [
            (
                "/work/app/compose.yaml",
                "services:\n  web:\n    image: web:${TAG}\n    restart: always\n",
            ),
            (
                "/work/app/prod/compose.prod.yaml",
                "services:\n  web:\n    image: web:${PROD_TAG}\n    volumes: [./conf:/conf]\n  db:\n    image: postgres\n",
            ),
            ("/work/app/.env", "TAG=1\nPROD_TAG=2\n"),
            ("/work/app/conf/x", ""),
        ];
        let request = files_request(&["app/compose.yaml", "app/prod/compose.prod.yaml"]);
        let project = load_files(&files, &request, &[]).unwrap();
        assert_eq!(
            project.files,
            ["app/compose.yaml", "app/prod/compose.prod.yaml"]
        );
        let web = project.service("web").unwrap();
        assert_eq!(web.location, at(2, 3));
        assert_eq!(image_tag(&project, "web").as_deref(), Some("2"));
        let second = |line, column| Location::new(FileId(1), line, column);
        assert_eq!(web.image.as_ref().unwrap().location, second(3, 12));
        assert_eq!(web.restart.as_ref().unwrap().location, at(4, 14));
        // Relative paths resolve against the first file's directory.
        assert!(matches!(&web.mounts[0], Mount::Bind { host_path, .. } if host_path.exists));
        assert_eq!(project.service("db").unwrap().location, second(5, 3));
    }

    #[test]
    fn unresolved_variables_are_located_in_their_own_file() {
        let files = [
            ("/work/a.yaml", "services:\n  web:\n    image: a:${A}\n"),
            ("/work/b.yaml", "services:\n  web:\n    restart: ${B}\n"),
        ];
        let project = load_files(&files, &files_request(&["a.yaml", "b.yaml"]), &[]).unwrap();
        let vars: Vec<_> = project
            .unresolved_vars
            .iter()
            .map(|var| (var.name.as_str(), var.location))
            .collect();
        assert_eq!(
            vars,
            [("A", at(3, 12)), ("B", Location::new(FileId(1), 3, 14))]
        );
    }

    #[test]
    fn errors_name_the_faulty_file() {
        let files = [
            ("/work/a.yaml", "services: {}\n"),
            ("/work/b.yaml", "services: [\n"),
        ];
        let error = load_files(&files, &files_request(&["a.yaml", "b.yaml"]), &[]).unwrap_err();
        assert!(
            matches!(error, LoadError::InvalidYaml { path, .. } if path == Path::new("b.yaml"))
        );
        let error = load_files(&files, &files_request(&["a.yaml", "c.yaml"]), &[]).unwrap_err();
        assert!(matches!(error, LoadError::NotFound { path } if path == Path::new("c.yaml")));
    }

    #[test]
    fn loads_files_reusing_few_anchors_many_times() {
        // Real-world files (Sentry self-hosted) reuse a handful of anchors
        // through hundreds of `<<: *defaults` aliases: valid YAML, not a bomb.
        let services: String = (0..120)
            .map(|index| format!("  s{index}:\n    <<: *defaults\n"))
            .collect();
        let content = format!("x-defaults: &defaults\n  image: nginx:1\nservices:\n{services}");
        let files = [("/work/compose.yaml", content.as_str())];
        let project = load_files(&files, &request(None), &[]).unwrap();
        assert_eq!(project.services.len(), 120);
    }
}
