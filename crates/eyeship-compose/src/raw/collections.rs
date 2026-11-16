//! Order-preserving collections that keep the location of their keys and items.
//!
//! `Spanned` is not `Ord`, so maps cannot be `BTreeMap`s; and untagged enums
//! lose positions, so the "list or map" shapes of Compose get hand-written
//! visitors.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_saphyr::Spanned;

use super::scalar::Scalar;

/// A located key, as read from a mapping.
pub(crate) type Key = Spanned<Scalar>;

/// A mapping in file order, keeping the location of each key.
#[derive(Debug)]
pub(crate) struct Entries<V>(pub(crate) Vec<(Key, V)>);

impl<V> Default for Entries<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

/// A Compose field written either as a list of names or as a mapping.
#[derive(Debug)]
pub(crate) enum ListOrMap<V> {
    List(Vec<Spanned<Scalar>>),
    Map(Vec<(Key, V)>),
}

/// A Compose field written either as a single scalar or as a list.
#[derive(Debug)]
pub(crate) enum OneOrMany<T> {
    One(Scalar),
    Many(Vec<Spanned<T>>),
}

fn collect_seq<'de, A, T>(mut seq: A) -> Result<Vec<T>, A::Error>
where
    A: SeqAccess<'de>,
    T: Deserialize<'de>,
{
    std::iter::from_fn(|| seq.next_element().transpose()).collect()
}

fn collect_map<'de, A, V>(mut map: A) -> Result<Vec<(Key, V)>, A::Error>
where
    A: MapAccess<'de>,
    V: Deserialize<'de>,
{
    std::iter::from_fn(|| map.next_entry().transpose()).collect()
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Entries<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(EntriesVisitor(PhantomData))
    }
}

struct EntriesVisitor<V>(PhantomData<V>);

impl<'de, V: Deserialize<'de>> Visitor<'de> for EntriesVisitor<V> {
    type Value = Entries<V>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a mapping")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Entries::default())
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        collect_map(map).map(Entries)
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for ListOrMap<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ListOrMapVisitor(PhantomData))
    }
}

struct ListOrMapVisitor<V>(PhantomData<V>);

impl<'de, V: Deserialize<'de>> Visitor<'de> for ListOrMapVisitor<V> {
    type Value = ListOrMap<V>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list or a mapping")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(ListOrMap::List(Vec::new()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        collect_seq(seq).map(ListOrMap::List)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        collect_map(map).map(ListOrMap::Map)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OneOrMany<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(OneOrManyVisitor(PhantomData))
    }
}

struct OneOrManyVisitor<T>(PhantomData<T>);

impl<T> OneOrManyVisitor<T> {
    fn one<E>(text: impl ToString) -> Result<OneOrMany<T>, E> {
        Ok(OneOrMany::One(Scalar(text.to_string())))
    }
}

impl<'de, T: Deserialize<'de>> Visitor<'de> for OneOrManyVisitor<T> {
    type Value = OneOrMany<T>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a scalar or a list")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Self::one(value)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Self::one(value)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Self::one(value)
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Self::one(value)
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Self::one(value)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(OneOrMany::Many(Vec::new()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        collect_seq(seq).map(OneOrMany::Many)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys<V>(entries: &[(Key, V)]) -> Vec<(String, u64, u64)> {
        entries
            .iter()
            .map(|(key, _)| {
                (
                    key.value.0.clone(),
                    key.referenced.line(),
                    key.referenced.column(),
                )
            })
            .collect()
    }

    #[test]
    fn entries_keep_file_order_and_key_locations() {
        let entries: Entries<Scalar> = serde_saphyr::from_str("zeta: 1\nalpha: 2\n").unwrap();
        assert_eq!(
            keys(&entries.0),
            [("zeta".to_owned(), 1, 1), ("alpha".to_owned(), 2, 1)]
        );
        assert_eq!(entries.0[1].1, Scalar("2".to_owned()));
    }

    #[test]
    fn list_or_map_reads_a_list_with_item_locations() {
        let parsed: ListOrMap<Scalar> = serde_saphyr::from_str("- a\n- b\n").unwrap();
        let ListOrMap::List(items) = parsed else {
            panic!("expected a list")
        };
        let lines: Vec<_> = items
            .iter()
            .map(|i| {
                (
                    i.value.0.as_str(),
                    i.referenced.line(),
                    i.referenced.column(),
                )
            })
            .collect();
        assert_eq!(lines, [("a", 1, 3), ("b", 2, 3)]);
    }

    #[test]
    fn list_or_map_reads_a_map() {
        let parsed: ListOrMap<Option<Scalar>> = serde_saphyr::from_str("a:\nb: 2\n").unwrap();
        let ListOrMap::Map(entries) = parsed else {
            panic!("expected a map")
        };
        assert_eq!(keys(&entries).len(), 2);
        assert_eq!(entries[0].1, None);
    }

    #[test]
    fn one_or_many_reads_a_scalar_or_a_list() {
        let one: OneOrMany<Scalar> = serde_saphyr::from_str("x").unwrap();
        assert!(matches!(one, OneOrMany::One(Scalar(s)) if s == "x"));
        let many: OneOrMany<Scalar> = serde_saphyr::from_str("[x, y]").unwrap();
        assert!(matches!(many, OneOrMany::Many(items) if items.len() == 2));
    }
}
