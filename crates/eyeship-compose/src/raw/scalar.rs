//! A Compose scalar: a string, a number or a boolean, kept as text.

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, Visitor};

/// A YAML scalar read as text, whatever its YAML type (`512`, `true`, `"x"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scalar(pub(crate) String);

impl Scalar {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Scalar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ScalarVisitor)
    }
}

struct ScalarVisitor;

impl Visitor<'_> for ScalarVisitor {
    type Value = Scalar;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string, a number or a boolean")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Scalar, E> {
        Ok(Scalar(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Scalar, E> {
        Ok(Scalar(value))
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Scalar, E> {
        Ok(Scalar(value.to_string()))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Scalar, E> {
        Ok(Scalar(value.to_string()))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Scalar, E> {
        Ok(Scalar(value.to_string()))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Scalar, E> {
        Ok(Scalar(value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<Vec<Scalar>, String> {
        serde_saphyr::from_str(yaml).map_err(|e| e.to_string())
    }

    fn scalars(values: &[&str]) -> Vec<Scalar> {
        values.iter().map(|v| Scalar((*v).to_owned())).collect()
    }

    #[test]
    fn accepts_strings_numbers_and_booleans() {
        assert_eq!(
            parse("[abc, \"8080:80\", 512, -1, 1.5, true]"),
            Ok(scalars(&["abc", "8080:80", "512", "-1", "1.5", "true"]))
        );
    }

    #[test]
    fn rejects_collections() {
        assert!(parse("[[1]]").is_err());
        assert!(parse("[{a: 1}]").is_err());
    }
}
