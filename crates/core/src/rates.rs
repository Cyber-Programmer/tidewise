//! Pure rate maths for live stats. No I/O, so it is easy to test.

use std::time::Duration;

/// Bytes per second between two cumulative counter readings.
/// `None` when there is no previous reading or no time has passed.
/// A counter that went backwards (wrap or reset) counts as zero, not a huge number.
pub fn rate_per_sec(previous: Option<u64>, current: u64, interval: Duration) -> Option<f64> {
    let previous = previous?;
    if interval.is_zero() {
        return None;
    }
    Some(current.saturating_sub(previous) as f64 / interval.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_SEC: Duration = Duration::from_secs(1);

    #[test]
    fn first_sample_has_no_rate() {
        assert_eq!(rate_per_sec(None, 1000, ONE_SEC), None);
    }

    #[test]
    fn zero_interval_has_no_rate() {
        assert_eq!(rate_per_sec(Some(0), 1000, Duration::ZERO), None);
    }

    #[test]
    fn steady_growth_is_bytes_per_second() {
        assert_eq!(rate_per_sec(Some(1000), 3000, ONE_SEC), Some(2000.0));
    }

    #[test]
    fn half_second_interval_doubles_the_rate() {
        let half = Duration::from_millis(500);
        assert_eq!(rate_per_sec(Some(0), 1000, half), Some(2000.0));
    }

    #[test]
    fn counter_wrap_counts_as_zero() {
        assert_eq!(rate_per_sec(Some(u64::MAX - 5), 10, ONE_SEC), Some(0.0));
    }

    #[test]
    fn unchanged_counter_is_zero() {
        assert_eq!(rate_per_sec(Some(42), 42, ONE_SEC), Some(0.0));
    }
}
