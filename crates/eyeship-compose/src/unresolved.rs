//! Root-level facts read from the generic YAML tree: unresolved variables
//! in every string, and the obsolete `version` key.

use eyeship_domain::{Location, UnresolvedVar};
use serde_saphyr::Spanned;

use crate::context::Context;
use crate::interpolate::interpolate;
use crate::raw::Node;

/// Every unresolved variable of the file, deduplicated by name and location.
///
/// Top-level `x-*` extensions are skipped: they only matter where they are
/// merged, and merged values are reported at the place they are used.
pub(crate) fn unresolved_vars(ctx: &Context<'_>, root: &Spanned<Node>) -> Vec<UnresolvedVar> {
    let Node::Map(sections) = &root.value else {
        return Vec::new();
    };
    sections
        .iter()
        .filter(|(key, _)| !key.value.as_str().starts_with("x-"))
        .flat_map(|(key, value)| match (key.value.as_str(), &value.value) {
            ("services", Node::Map(services)) => services
                .iter()
                .flat_map(|(name, service)| scan(ctx, service, Some(name.value.as_str())))
                .collect(),
            _ => scan(ctx, value, None),
        })
        .fold(Vec::new(), |mut found: Vec<UnresolvedVar>, var| {
            if !found
                .iter()
                .any(|known| known.name == var.name && known.location == var.location)
            {
                found.push(var);
            }
            found
        })
}

/// The location of the root `version` key, if present.
pub(crate) fn version_key(ctx: &Context<'_>, root: &Spanned<Node>) -> Option<Location> {
    match &root.value {
        Node::Map(sections) => sections
            .iter()
            .find(|(key, _)| key.value.as_str() == "version")
            .map(|(key, _)| ctx.location(&key.referenced)),
        _ => None,
    }
}

fn scan(ctx: &Context<'_>, node: &Spanned<Node>, service: Option<&str>) -> Vec<UnresolvedVar> {
    texts(node)
        .into_iter()
        .flat_map(|(text, position)| {
            interpolate(text, &|name| ctx.vars.lookup(name))
                .unresolved
                .into_iter()
                .map(move |unresolved| UnresolvedVar {
                    name: unresolved.name,
                    service: service.map(str::to_owned),
                    location: ctx.location(position),
                })
        })
        .collect()
}

/// Every string of a tree, with its position, in document order.
fn texts(node: &Spanned<Node>) -> Vec<(&str, &serde_saphyr::Location)> {
    match &node.value {
        Node::Text(text) => vec![(text.as_str(), &node.referenced)],
        Node::Other => Vec::new(),
        Node::Seq(items) => items.iter().flat_map(texts).collect(),
        Node::Map(entries) => entries.iter().flat_map(|(_, value)| texts(value)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::test_support::*;

    fn vars(yaml: &str) -> Vec<(String, Option<String>, u32, u32)> {
        let root: Spanned<Node> = serde_saphyr::from_str(yaml).unwrap();
        with_context(&[], &[("SET", "1")], |ctx| unresolved_vars(ctx, &root))
            .into_iter()
            .map(|v| (v.name, v.service, v.location.line, v.location.column))
            .collect()
    }

    fn var(
        name: &str,
        service: Option<&str>,
        line: u32,
        column: u32,
    ) -> (String, Option<String>, u32, u32) {
        (name.to_owned(), service.map(str::to_owned), line, column)
    }

    #[test]
    fn finds_variables_in_every_string_of_a_service() {
        let yaml = "services:
  api:
    image: app:${TAG}
    environment:
      - URL=$URL
      - OK=${SET}
      - DEF=${NOPE:-x}
    command: [\"echo\", \"${ARG:?ARG is required}\"]
";
        assert_eq!(
            vars(yaml),
            [
                var("TAG", Some("api"), 3, 12),
                var("URL", Some("api"), 5, 9),
                var("ARG", Some("api"), 8, 23),
            ]
        );
    }

    #[test]
    fn root_sections_have_no_service_and_extensions_are_skipped() {
        let yaml = "x-defaults:\n  image: ${IGNORED}\nnetworks:\n  front:\n    name: ${NET}\n";
        assert_eq!(vars(yaml), [var("NET", None, 5, 11)]);
    }

    #[test]
    fn duplicates_at_the_same_place_are_reported_once() {
        let yaml = "services:\n  a:\n    image: ${X}-${X}-${Y}\n";
        assert_eq!(
            vars(yaml),
            [var("X", Some("a"), 3, 12), var("Y", Some("a"), 3, 12)]
        );
    }

    #[test]
    fn version_key_location() {
        let root: Spanned<Node> =
            serde_saphyr::from_str("services: {}\nversion: \"3.8\"\n").unwrap();
        assert_eq!(
            with_context(&[], &[], |ctx| version_key(ctx, &root)),
            Some(at(2, 1))
        );
        let root: Spanned<Node> = serde_saphyr::from_str("services: {}\n").unwrap();
        assert_eq!(with_context(&[], &[], |ctx| version_key(ctx, &root)), None);
    }
}
