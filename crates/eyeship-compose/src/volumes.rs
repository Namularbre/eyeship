//! Normalization of a service's `volumes`, short and long syntax.

use std::iter::once;

use eyeship_domain::{HostPathKind, Location, Mount};
use serde_saphyr::Spanned;

use crate::context::{Context, has_windows_drive};
use crate::raw::{RawVolume, Scalar};

/// Options accepted after the second `:` of the short syntax.
const KNOWN_MODES: [&str; 14] = [
    "ro",
    "rw",
    "z",
    "Z",
    "cached",
    "delegated",
    "consistent",
    "nocopy",
    "rprivate",
    "private",
    "rshared",
    "shared",
    "rslave",
    "slave",
];

/// Mount types that need no declaration and no host path.
const ANONYMOUS_TYPES: [&str; 4] = ["tmpfs", "npipe", "cluster", "image"];

/// Maps every entry of a service's `volumes`; non-scalar, non-mapping
/// entries are skipped.
pub(crate) fn map_volumes(ctx: &Context<'_>, entries: &[Spanned<RawVolume>]) -> Vec<Mount> {
    entries
        .iter()
        .filter_map(|entry| map_volume(ctx, &entry.value, ctx.location(&entry.referenced)))
        .collect()
}

fn map_volume(ctx: &Context<'_>, volume: &RawVolume, location: Location) -> Option<Mount> {
    let text = |scalar: &Option<Scalar>| {
        scalar
            .as_ref()
            .map(|s| ctx.interpolate(s.as_str()))
            .unwrap_or_default()
    };
    let mount = match volume {
        RawVolume::Short(spec) => parse_short(ctx, &ctx.interpolate(spec.as_str()), location),
        RawVolume::Long {
            kind,
            source,
            target,
        } => map_long(
            ctx,
            &LongVolume {
                kind: kind
                    .as_ref()
                    .map_or_else(|| "volume".to_owned(), |k| ctx.interpolate(k.as_str())),
                source: text(source),
                target: text(target),
            },
            location,
        ),
        RawVolume::Other(_) => return None,
    };
    match mount {
        // R013 already reports the unresolved variable that broke the entry.
        Mount::Malformed { .. } if has_unresolved(ctx, volume) => None,
        mount => Some(mount),
    }
}

/// Whether any field of the entry references a variable with no value.
fn has_unresolved(ctx: &Context<'_>, volume: &RawVolume) -> bool {
    match volume {
        RawVolume::Short(spec) => ctx.has_unresolved(spec.as_str()),
        RawVolume::Long {
            kind,
            source,
            target,
        } => [kind, source, target]
            .into_iter()
            .flatten()
            .any(|scalar| ctx.has_unresolved(scalar.as_str())),
        RawVolume::Other(_) => false,
    }
}

fn malformed(raw: &str, reason: impl Into<String>, location: Location) -> Mount {
    Mount::Malformed {
        raw: raw.to_owned(),
        reason: reason.into(),
        location,
    }
}

/// Parses `[source:]target[:mode]`.
fn parse_short(ctx: &Context<'_>, spec: &str, location: Location) -> Mount {
    match split_volume(spec).as_slice() {
        [target] => checked_target(spec, target, location).unwrap_or_else(|| Mount::Anonymous {
            target: (*target).to_owned(),
            location,
        }),
        [source, target] => source_mount(ctx, spec, source, target, location),
        [source, target, mode] => match unknown_mode(mode) {
            Some(option) => malformed(
                spec,
                format!(
                    "unknown volume option '{option}'; expected a comma-separated list of {}",
                    KNOWN_MODES.join(", ")
                ),
                location,
            ),
            None => source_mount(ctx, spec, source, target, location),
        },
        _ => malformed(
            spec,
            "too many ':' separators; expected source:target[:mode]",
            location,
        ),
    }
}

/// Splits on `:`, except the colon of a Windows drive such as `C:\`.
fn split_volume(spec: &str) -> Vec<&str> {
    let separators: Vec<usize> = spec
        .match_indices(':')
        .map(|(index, _)| index)
        .filter(|&index| !is_drive_colon(spec, index))
        .collect();
    let starts = once(0).chain(separators.iter().map(|index| index + 1));
    let ends = separators.iter().copied().chain(once(spec.len()));
    starts
        .zip(ends)
        .map(|(start, end)| &spec[start..end])
        .collect()
}

/// Whether the colon at `index` follows a lone drive letter and precedes `\`.
fn is_drive_colon(spec: &str, index: usize) -> bool {
    let bytes = spec.as_bytes();
    let letter_starts_part = index == 1 || (index >= 2 && bytes[index - 2] == b':');
    index >= 1
        && letter_starts_part
        && bytes[index - 1].is_ascii_alphabetic()
        && bytes.get(index + 1) == Some(&b'\\')
}

fn unknown_mode(mode: &str) -> Option<&str> {
    mode.split(',').find(|option| !KNOWN_MODES.contains(option))
}

/// A malformed mount when the container path is empty or relative.
fn checked_target(raw: &str, target: &str, location: Location) -> Option<Mount> {
    if target.is_empty() {
        Some(malformed(raw, "the container path is empty", location))
    } else if target.starts_with('/') || has_windows_drive(target) {
        None
    } else {
        Some(malformed(
            raw,
            format!("the container path '{target}' must be absolute"),
            location,
        ))
    }
}

fn source_mount(
    ctx: &Context<'_>,
    raw: &str,
    source: &str,
    target: &str,
    location: Location,
) -> Mount {
    if source.is_empty() {
        return malformed(raw, "the volume source is empty", location);
    }
    checked_target(raw, target, location).unwrap_or_else(|| {
        if is_bind_source(source) {
            Mount::Bind {
                host_path: ctx.host_path(source.to_owned(), HostPathKind::Bind, location),
                target: target.to_owned(),
            }
        } else {
            Mount::Named {
                name: source.to_owned(),
                target: target.to_owned(),
                location,
            }
        }
    })
}

/// Compose treats a source as a host path when it looks like a path.
fn is_bind_source(source: &str) -> bool {
    source.starts_with(['.', '/', '~', '\\']) || has_windows_drive(source)
}

/// The interpolated fields of a long-syntax volume.
struct LongVolume {
    kind: String,
    source: String,
    target: String,
}

fn map_long(ctx: &Context<'_>, volume: &LongVolume, location: Location) -> Mount {
    let raw = format!(
        "type={},source={},target={}",
        volume.kind, volume.source, volume.target
    );
    let target = volume.target.clone();
    if let Some(problem) = checked_target(&raw, &target, location) {
        return problem;
    }
    match volume.kind.as_str() {
        "volume" if volume.source.is_empty() => Mount::Anonymous { target, location },
        "volume" => Mount::Named {
            name: volume.source.clone(),
            target,
            location,
        },
        "bind" if volume.source.is_empty() => {
            malformed(&raw, "a bind mount needs a source", location)
        }
        "bind" => Mount::Bind {
            host_path: ctx.host_path(volume.source.clone(), HostPathKind::Bind, location),
            target,
        },
        kind if ANONYMOUS_TYPES.contains(&kind) => Mount::Anonymous { target, location },
        kind => malformed(
            &raw,
            format!("unknown mount type '{kind}'; expected volume, bind, tmpfs, npipe or cluster"),
            location,
        ),
    }
}

#[cfg(test)]
mod tests {
    use eyeship_domain::HostPath;

    use super::*;
    use crate::context::test_support::*;

    fn mounts(yaml: &str) -> Vec<Mount> {
        with_context(&["/app/data/file"], &[("DATA", "./data")], |ctx| {
            map_volumes(ctx, &service(yaml).volumes.unwrap_or_default())
        })
    }

    fn one(entry: &str) -> Mount {
        let all = mounts(&format!("volumes: [{entry}]"));
        assert_eq!(all.len(), 1, "{entry}");
        all.into_iter().next().unwrap()
    }

    fn named(name: &str, target: &str) -> Mount {
        Mount::Named {
            name: name.into(),
            target: target.into(),
            location: at(1, 11),
        }
    }

    fn bind(path: &str, exists: bool, target: &str) -> Mount {
        Mount::Bind {
            host_path: HostPath {
                path: path.into(),
                kind: HostPathKind::Bind,
                exists,
                location: at(1, 11),
            },
            target: target.into(),
        }
    }

    fn reason(mount: &Mount) -> Option<&str> {
        match mount {
            Mount::Malformed { reason, .. } => Some(reason),
            _ => None,
        }
    }

    #[test]
    fn named_volumes() {
        assert_eq!(one("\"db:/var/lib/db\""), named("db", "/var/lib/db"));
        assert_eq!(one("\"db:/var/lib/db:ro\""), named("db", "/var/lib/db"));
    }

    #[test]
    fn bind_mounts_check_relative_paths() {
        assert_eq!(one("\"./data:/data\""), bind("./data", true, "/data"));
        assert_eq!(one("\"./nope:/data:ro,z\""), bind("./nope", false, "/data"));
        assert_eq!(one("\"${DATA}:/data\""), bind("./data", true, "/data"));
    }

    #[test]
    fn bind_mounts_on_absolute_paths_are_assumed_to_exist() {
        assert_eq!(one("\"/srv/x:/x\""), bind("/srv/x", true, "/x"));
        assert_eq!(one("\"~/x:/y\""), bind("~/x", true, "/y"));
        assert_eq!(one("'C:\\data:/data'"), bind("C:\\data", true, "/data"));
    }

    #[test]
    fn anonymous_volumes() {
        assert_eq!(
            one("/cache"),
            Mount::Anonymous {
                target: "/cache".into(),
                location: at(1, 11)
            }
        );
    }

    #[test]
    fn unknown_modes_are_malformed() {
        assert!(reason(&one("\"db:/data:rw,bogus\"")).is_some_and(|r| r.contains("'bogus'")));
        assert!(reason(&one("\"db:/data:\"")).is_some());
    }

    #[test]
    fn too_many_colons_are_malformed() {
        assert!(reason(&one("\"a:/b:ro:x\"")).is_some_and(|r| r.contains("too many")));
    }

    #[test]
    fn empty_or_relative_targets_are_malformed() {
        assert!(reason(&one("\":/data\"")).is_some());
        assert!(reason(&one("\"db:\"")).is_some());
        assert!(reason(&one("\"db:data\"")).is_some_and(|r| r.contains("absolute")));
    }

    #[test]
    fn long_syntax() {
        let yaml = "volumes:
  - {type: volume, source: db, target: /db}
  - {type: bind, source: ./nope, target: /x}
  - {type: tmpfs, target: /tmp}
  - {source: db2, target: /db2}
  - {type: volume, target: /anon}
  - {type: weird, target: /w}
  - {type: bind, target: /b}
  - [x]
";
        let all = mounts(yaml);
        assert_eq!(all.len(), 7);
        assert!(matches!(&all[0], Mount::Named { name, .. } if name == "db"));
        assert!(matches!(&all[1], Mount::Bind { host_path, .. } if !host_path.exists));
        assert!(matches!(&all[2], Mount::Anonymous { target, .. } if target == "/tmp"));
        assert!(matches!(&all[3], Mount::Named { name, .. } if name == "db2"));
        assert!(matches!(&all[4], Mount::Anonymous { .. }));
        assert!(reason(&all[5]).is_some_and(|r| r.contains("weird")));
        assert!(reason(&all[6]).is_some());
        assert_eq!(all[1].location(), at(3, 5));
    }

    #[test]
    fn entries_broken_only_by_an_unresolved_variable_are_left_to_r013() {
        // `${UPLOAD_LOCATION}:/data` with no `.env`: R013 already reports the
        // variable; calling the entry malformed would double-report it.
        assert!(mounts("volumes: [\"${UNSET}:/data\"]").is_empty());
        assert!(mounts("volumes: [\"db:${UNSET}\"]").is_empty());
        assert!(mounts("volumes: [{type: bind, source: \"${UNSET}\", target: /b}]").is_empty());
        assert!(reason(&one("\"${DATA}:/data:bogus\"")).is_some());
    }
}
