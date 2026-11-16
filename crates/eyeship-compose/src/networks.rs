//! Networks: declared at the root and used by services.

use eyeship_domain::{Declared, Located};
use serde::de::IgnoredAny;

use crate::context::Context;
use crate::raw::{Entries, ListOrMap, RawDeclared, RawExternal};

/// Maps a service's `networks`, list or map form.
pub(crate) fn map_service_networks(
    ctx: &Context<'_>,
    networks: &ListOrMap<IgnoredAny>,
) -> Vec<Located<String>> {
    match networks {
        // A name broken by an unresolved variable is left to R013.
        ListOrMap::List(names) => names
            .iter()
            .filter_map(|name| ctx.resolved_text(name))
            .collect(),
        ListOrMap::Map(entries) => entries
            .iter()
            .map(|(name, _)| {
                Located::new(
                    name.value.as_str().to_owned(),
                    ctx.location(&name.referenced),
                )
            })
            .collect(),
    }
}

/// Maps the root `networks` or `volumes` section.
pub(crate) fn map_declared(
    ctx: &Context<'_>,
    declared: &Entries<Option<RawDeclared>>,
) -> Vec<Declared> {
    declared
        .0
        .iter()
        .map(|(name, body)| Declared {
            name: name.value.as_str().to_owned(),
            external: body
                .as_ref()
                .and_then(|body| body.external.as_ref())
                .is_some_and(|external| is_external(ctx, external)),
            location: ctx.location(&name.referenced),
        })
        .collect()
}

fn is_external(ctx: &Context<'_>, external: &RawExternal) -> bool {
    match external {
        RawExternal::Flag(flag) => ctx.interpolate(flag.as_str()) == "true",
        RawExternal::Legacy(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::test_support::*;

    fn service_networks(yaml: &str) -> Vec<Located<String>> {
        with_context(&[], &[], |ctx| {
            service(yaml)
                .networks
                .map(|n| map_service_networks(ctx, &n))
                .unwrap_or_default()
        })
    }

    #[test]
    fn service_networks_list_form() {
        assert_eq!(
            service_networks("networks: [front, back]"),
            [
                Located::new("front".to_owned(), at(1, 12)),
                Located::new("back".to_owned(), at(1, 19))
            ]
        );
    }

    #[test]
    fn service_networks_map_form() {
        assert_eq!(
            service_networks("networks:\n  front:\n    aliases: [x]\n  back:\n"),
            [
                Located::new("front".to_owned(), at(2, 3)),
                Located::new("back".to_owned(), at(4, 3))
            ]
        );
    }

    #[test]
    fn declared_with_external_flag() {
        let yaml =
            "a:\nb:\n  external: true\nc:\n  external: false\nd:\n  external:\n    name: x\n";
        let entries: Entries<Option<RawDeclared>> = serde_saphyr::from_str(yaml).unwrap();
        let declared = with_context(&[], &[], |ctx| map_declared(ctx, &entries));
        let summary: Vec<_> = declared
            .iter()
            .map(|d| (d.name.as_str(), d.external, d.location.line))
            .collect();
        assert_eq!(
            summary,
            [
                ("a", false, 1),
                ("b", true, 2),
                ("c", false, 4),
                ("d", true, 6)
            ]
        );
    }

    #[test]
    fn service_networks_with_an_unresolved_variable_are_left_to_r013() {
        assert_eq!(
            service_networks("networks: [\"${UNSET}\", \"app-${UNSET}\", front]"),
            [Located::new("front".to_owned(), at(1, 40))]
        );
    }
}
