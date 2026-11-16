//! Conversion of YAML positions to domain locations.

use eyeship_domain::{FileId, Location};

/// Converts a `serde_saphyr` position into a domain location in `file`.
pub(crate) fn to_location(file: FileId, position: &serde_saphyr::Location) -> Location {
    Location::new(
        file,
        u32::try_from(position.line()).unwrap_or(u32::MAX),
        u32::try_from(position.column()).unwrap_or(u32::MAX),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_saphyr::Spanned;

    #[test]
    fn keeps_line_and_column() {
        let value: Spanned<String> = serde_saphyr::from_str("\n  hello").unwrap();
        assert_eq!(
            to_location(FileId(3), &value.referenced),
            Location::new(FileId(3), 2, 3)
        );
    }
}
