//! Compose byte sizes: `512`, `512m`, `1.5g`, `64KB`.

/// Parses a byte size, with an optional case-insensitive unit
/// (`b`, `k`/`kb`, `m`/`mb`, `g`/`gb`, `t`/`tb`, binary multiples).
pub(crate) fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let multiplier = unit_multiplier(unit.trim())?;
    let number: f64 = number.parse().ok()?;
    let bytes = (number * multiplier).round();
    (bytes.is_finite() && bytes >= 0.0 && bytes <= u64::MAX as f64).then_some(bytes as u64)
}

fn unit_multiplier(unit: &str) -> Option<f64> {
    let exponent = match unit {
        "" | "b" => 0,
        "k" | "kb" => 1,
        "m" | "mb" => 2,
        "g" | "gb" => 3,
        "t" | "tb" => 4,
        _ => return None,
    };
    Some(1024f64.powi(exponent))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn bare_numbers_are_bytes() {
        assert_eq!(parse_size("536870912"), Some(512 * MIB));
        assert_eq!(parse_size("100b"), Some(100));
    }

    #[test]
    fn units_are_binary_multiples() {
        assert_eq!(parse_size("64k"), Some(64 * 1024));
        assert_eq!(parse_size("64kb"), Some(64 * 1024));
        assert_eq!(parse_size("512m"), Some(512 * MIB));
        assert_eq!(parse_size("512mb"), Some(512 * MIB));
        assert_eq!(parse_size("1g"), Some(1024 * MIB));
        assert_eq!(parse_size("2gb"), Some(2048 * MIB));
    }

    #[test]
    fn units_are_case_insensitive() {
        assert_eq!(parse_size("512M"), Some(512 * MIB));
        assert_eq!(parse_size("1GB"), Some(1024 * MIB));
    }

    #[test]
    fn decimals_are_accepted() {
        assert_eq!(parse_size("1.5g"), Some(1536 * MIB));
    }

    #[test]
    fn invalid_sizes_are_rejected() {
        ["", "m", "12x", "abc", "1.2.3m", "-5m"]
            .iter()
            .for_each(|text| assert_eq!(parse_size(text), None, "{text}"));
    }
}
