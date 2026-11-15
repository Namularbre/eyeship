//! A generic YAML tree that keeps the location of every node.
//!
//! Used to find unresolved variables in every string of the file, including
//! fields the typed model does not read.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_saphyr::Spanned;

use super::collections::Key;

/// A YAML node: only strings are kept, other scalars become `Other`.
#[derive(Debug)]
pub(crate) enum Node {
    Text(String),
    Other,
    Seq(Vec<Spanned<Node>>),
    Map(Vec<(Key, Spanned<Node>)>),
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NodeVisitor)
    }
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any YAML value")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Node, E> {
        Ok(Node::Text(value.to_owned()))
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_none<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node::Other)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        std::iter::from_fn(|| seq.next_element().transpose())
            .collect::<Result<_, _>>()
            .map(Node::Seq)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        std::iter::from_fn(|| map.next_entry().transpose())
            .collect::<Result<_, _>>()
            .map(Node::Map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_any_document_with_locations() {
        let node: Spanned<Node> =
            serde_saphyr::from_str("a:\n  - x\n  - 1\n  - ~\n  - true\nb: {c: 1.5}\n").unwrap();
        let Node::Map(entries) = node.value else {
            panic!("expected a map")
        };
        let Node::Seq(items) = &entries[0].1.value else {
            panic!("expected a sequence")
        };
        assert!(matches!(&items[0].value, Node::Text(t) if t == "x"));
        assert_eq!(
            (items[0].referenced.line(), items[0].referenced.column()),
            (2, 5)
        );
        assert!(items[1..].iter().all(|i| matches!(i.value, Node::Other)));
        assert!(matches!(&entries[1].1.value, Node::Map(m) if m.len() == 1));
    }
}
