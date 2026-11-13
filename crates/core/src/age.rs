//! How old something is, and a plain UTC timestamp. No clock in here: the caller passes "now".
//! Library code must not panic, so no indexing or `unwrap` outside the tests.

use std::time::SystemTime;

/// Whole days from `earlier` to `now` (a part of a day does not count).
/// 0 when `earlier` is in the future (a clock that is off must not give a huge age).
pub fn whole_days(_earlier: SystemTime, _now: SystemTime) -> u64 {
    // Stub for the red commit.
    0
}

/// `2026-10-14 08:12:03 UTC`. Times before 1970 show as 1970-01-01 00:00:00 UTC.
pub fn utc_timestamp(_time: SystemTime) -> String {
    // Stub for the red commit.
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    const DAY: u64 = 86_400;

    #[test]
    fn the_same_moment_is_zero_days_old() {
        assert_eq!(whole_days(at(1_000), at(1_000)), 0);
    }

    #[test]
    fn part_of_a_day_does_not_count() {
        assert_eq!(whole_days(at(0), at(DAY - 1)), 0);
        assert_eq!(whole_days(at(0), at(DAY)), 1);
        assert_eq!(whole_days(at(0), at(2 * DAY - 1)), 1);
    }

    #[test]
    fn thirty_days_is_thirty() {
        assert_eq!(whole_days(at(5_000), at(5_000 + 30 * DAY)), 30);
    }

    #[test]
    fn a_time_in_the_future_is_zero_days_old() {
        assert_eq!(whole_days(at(10 * DAY), at(DAY)), 0);
    }

    #[test]
    fn the_epoch_is_formatted() {
        assert_eq!(utc_timestamp(at(0)), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc_timestamp(at(86_399)), "1970-01-01 23:59:59 UTC");
        assert_eq!(utc_timestamp(at(86_400)), "1970-01-02 00:00:00 UTC");
    }

    #[test]
    fn ordinary_dates_are_formatted() {
        assert_eq!(utc_timestamp(at(1_700_000_000)), "2023-11-14 22:13:20 UTC");
        assert_eq!(utc_timestamp(at(1_760_400_000)), "2025-10-14 00:00:00 UTC");
    }

    #[test]
    fn leap_days_and_century_ends_are_right() {
        // 2000 was a leap year (divisible by 400).
        assert_eq!(utc_timestamp(at(951_782_400)), "2000-02-29 00:00:00 UTC");
        assert_eq!(utc_timestamp(at(951_868_799)), "2000-02-29 23:59:59 UTC");
        assert_eq!(utc_timestamp(at(4_102_444_799)), "2099-12-31 23:59:59 UTC");
        assert_eq!(utc_timestamp(at(4_102_444_800)), "2100-01-01 00:00:00 UTC");
    }

    #[test]
    fn a_time_before_1970_shows_as_the_epoch() {
        assert_eq!(
            utc_timestamp(UNIX_EPOCH - Duration::from_secs(5)),
            "1970-01-01 00:00:00 UTC"
        );
    }
}
