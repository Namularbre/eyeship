//! Compose durations: `1m30s`, `10s`, `500ms`, `1h`, `2us`, `1.5s`.

use std::time::Duration;

/// Parses a Go-style duration as used by Compose. `0` alone is accepted.
pub(crate) fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    if text == "0" {
        return Some(Duration::ZERO);
    }
    let nanos = segments(text)?.round();
    (nanos.is_finite() && nanos <= u64::MAX as f64).then(|| Duration::from_nanos(nanos as u64))
}

/// Sums every `<number><unit>` segment of `text`, in nanoseconds.
fn segments(text: &str) -> Option<f64> {
    let (nanos, rest) = segment(text)?;
    if rest.is_empty() {
        Some(nanos)
    } else {
        segments(rest).map(|more| nanos + more)
    }
}

/// Reads one `<number><unit>` segment; returns its nanoseconds and the rest.
fn segment(text: &str) -> Option<(f64, &str)> {
    let number_len = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let (number, rest) = text.split_at(number_len);
    let unit_len = rest
        .find(|c: char| c.is_ascii_digit() || c == '.')
        .unwrap_or(rest.len());
    let (unit, rest) = rest.split_at(unit_len);
    let value: f64 = number.parse().ok()?;
    Some((value * unit_nanos(unit)?, rest))
}

fn unit_nanos(unit: &str) -> Option<f64> {
    match unit {
        "ns" => Some(1.0),
        "us" | "µs" | "μs" => Some(1e3),
        "ms" => Some(1e6),
        "s" => Some(1e9),
        "m" => Some(60e9),
        "h" => Some(3600e9),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(text: &str) -> Option<f64> {
        parse_duration(text).map(|d| d.as_secs_f64())
    }

    #[test]
    fn single_units() {
        assert_eq!(parse_duration("10s"), Some(Duration::from_secs(10)));
        assert_eq!(parse_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration("2us"), Some(Duration::from_micros(2)));
        assert_eq!(parse_duration("3m"), Some(Duration::from_secs(180)));
    }

    #[test]
    fn combined_units() {
        assert_eq!(parse_duration("1m30s"), Some(Duration::from_secs(90)));
        assert_eq!(secs("1h2m3s"), Some(3723.0));
    }

    #[test]
    fn decimals_and_zero() {
        assert_eq!(parse_duration("1.5s"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_duration("0"), Some(Duration::ZERO));
    }

    #[test]
    fn invalid_durations_are_rejected() {
        ["", "10", "s", "10x", "1m30", "abc", "-5s"]
            .iter()
            .for_each(|text| assert_eq!(parse_duration(text), None, "{text}"));
    }
}
