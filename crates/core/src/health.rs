//! The health score and its rules, exactly as the "Why your score is 82" artboard states them:
//! every PC starts at 100, each problem takes off a fixed number of points, nothing else counts.
//!
//! Real today: drive fullness. Junk is a stub until the cleaner exists (M3): callers pass `None`
//! and no points are taken off. Services and updates are not measured yet either.

/// −2 points for every 5 % above 60 % used (whole steps only).
pub fn disk_points(used_fraction: f64) -> u32 {
    if !used_fraction.is_finite() {
        return 0;
    }
    // Whole tenths of a percent, so 0.65 is exactly 650 and not 649.999….
    let tenths = (used_fraction.clamp(0.0, 1.0) * 1000.0).round() as u32;
    tenths.saturating_sub(600) / 50 * 2
}

/// −1 point per 2.5 GB of junk, at most −8.
pub fn junk_points(junk_bytes: u64) -> u32 {
    (junk_bytes / 2_500_000_000).min(8) as u32
}

/// 100 minus the points, never below 0.
pub fn score(points: &[u32]) -> u32 {
    let lost = points.iter().fold(0u32, |sum, p| sum.saturating_add(*p));
    100u32.saturating_sub(lost)
}

/// The word under the ring.
pub fn tag(score: u32) -> &'static str {
    match score {
        80.. => "Good shape",
        60..=79 => "Could be better",
        _ => "Needs attention",
    }
}

/// Is one app using enough memory to be worth a suggestion? From 1 GB up.
pub fn is_big_memory(app_bytes: u64) -> bool {
    app_bytes >= 1_000_000_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_points_follow_the_artboard_rule() {
        assert_eq!(disk_points(0.0), 0);
        assert_eq!(disk_points(0.5), 0);
        assert_eq!(disk_points(0.60), 0, "60 % is not above 60 %");
        assert_eq!(disk_points(0.649), 0, "a step needs the full 5 %");
        assert_eq!(disk_points(0.65), 2);
        assert_eq!(disk_points(0.699), 2);
        assert_eq!(disk_points(0.70), 4);
        assert_eq!(disk_points(0.80), 8, "the artboard: 80 % used is −8");
        assert_eq!(disk_points(0.95), 14);
        assert_eq!(disk_points(1.0), 16);
    }

    #[test]
    fn disk_points_survive_nonsense() {
        assert_eq!(disk_points(f64::NAN), 0);
        assert_eq!(disk_points(-1.0), 0);
        assert_eq!(disk_points(7.0), 16, "over 100 % counts as 100 %");
    }

    #[test]
    fn junk_points_follow_the_artboard_rule() {
        assert_eq!(junk_points(0), 0);
        assert_eq!(junk_points(2_499_999_999), 0);
        assert_eq!(junk_points(2_500_000_000), 1);
        assert_eq!(junk_points(12_600_000_000), 5, "the artboard: 12.6 GB is −5");
        assert_eq!(junk_points(20_000_000_000), 8);
        assert_eq!(junk_points(500_000_000_000), 8, "at most −8");
        assert_eq!(junk_points(u64::MAX), 8);
    }

    #[test]
    fn the_score_starts_at_100_and_never_goes_below_0() {
        assert_eq!(score(&[]), 100);
        assert_eq!(score(&[8, 5, 3, 2]), 82, "the artboard's 82");
        assert_eq!(score(&[60, 60]), 0);
        assert_eq!(score(&[u32::MAX, u32::MAX]), 0);
    }

    #[test]
    fn the_tag_has_three_bands() {
        assert_eq!(tag(100), "Good shape");
        assert_eq!(tag(80), "Good shape");
        assert_eq!(tag(79), "Could be better");
        assert_eq!(tag(60), "Could be better");
        assert_eq!(tag(59), "Needs attention");
        assert_eq!(tag(0), "Needs attention");
    }

    #[test]
    fn one_gigabyte_is_the_memory_suggestion_line() {
        assert!(!is_big_memory(999_999_999));
        assert!(is_big_memory(1_000_000_000));
        assert!(is_big_memory(2_100_000_000));
    }
}
