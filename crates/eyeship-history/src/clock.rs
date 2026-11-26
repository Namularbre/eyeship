//! Dates without a date library (plan D7): UTC only, to the millisecond.

use std::time::{SystemTime, UNIX_EPOCH};

/// Where the current time comes from, so that tests can fix it.
pub trait Clock {
    /// Milliseconds since the Unix epoch.
    fn now_millis(&self) -> u64;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
            })
    }
}

/// A clock stopped at a given instant, for tests.
#[derive(Debug, Clone, Copy, Default)]
pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now_millis(&self) -> u64 {
        self.0
    }
}

/// Calendar fields of an instant, in UTC.
struct DateTime {
    year: i64,
    month: u32,
    day: u32,
    hour: u64,
    minute: u64,
    second: u64,
    millis: u64,
}

fn date_time(millis: u64) -> DateTime {
    let seconds = millis / 1000;
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    DateTime {
        year,
        month,
        day,
        hour: of_day / 3600,
        minute: of_day % 3600 / 60,
        second: of_day % 60,
        millis: millis % 1000,
    }
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian calendar.
/// Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

/// `2026-11-26T11:47:17.123Z`.
pub fn rfc3339(millis: u64) -> String {
    let t = date_time(millis);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year, t.month, t.day, t.hour, t.minute, t.second, t.millis
    )
}

/// `2026-11-26T11-47-17.123Z`: the date as a file name, without `:` so that
/// it is valid on Windows, and sorting by name sorts by date.
pub fn file_id(millis: u64) -> String {
    rfc3339(millis).replace(':', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400_000;

    #[test]
    fn formats_the_epoch() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn formats_a_recent_date_to_the_millisecond() {
        assert_eq!(rfc3339(1_764_157_637_123), "2025-11-26T11:47:17.123Z");
        assert_eq!(file_id(1_764_157_637_123), "2025-11-26T11-47-17.123Z");
    }

    #[test]
    fn handles_leap_years_and_month_ends() {
        // 2024-02-28, 2024-02-29, 2024-03-01
        let feb_28 = 1_709_078_400_000;
        assert_eq!(&rfc3339(feb_28)[..10], "2024-02-28");
        assert_eq!(&rfc3339(feb_28 + DAY)[..10], "2024-02-29");
        assert_eq!(&rfc3339(feb_28 + 2 * DAY)[..10], "2024-03-01");
        // 2100 is not a leap year, 2000 is.
        assert_eq!(&rfc3339(4_107_456_000_000)[..10], "2100-02-28");
        assert_eq!(&rfc3339(4_107_456_000_000 + DAY)[..10], "2100-03-01");
        assert_eq!(&rfc3339(951_782_400_000)[..10], "2000-02-29");
        assert_eq!(rfc3339(1_767_225_599_999), "2025-12-31T23:59:59.999Z");
        assert_eq!(rfc3339(1_767_225_600_000), "2026-01-01T00:00:00.000Z");
    }

    #[test]
    fn file_ids_sort_like_dates() {
        let ids: Vec<String> = [0, 999, 1000, DAY, 400 * DAY].map(file_id).to_vec();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        assert!(ids.iter().all(|id| !id.contains(':')));
    }

    #[test]
    fn the_system_clock_is_after_2025() {
        assert!(SystemClock.now_millis() > 1_735_689_600_000);
        assert_eq!(FixedClock(42).now_millis(), 42);
    }
}
