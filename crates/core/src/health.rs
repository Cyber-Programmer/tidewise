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

/// −3 points for every failed service.
pub const SERVICE_POINTS_EACH: u32 = 3;

pub fn service_points(_failed: usize) -> u32 {
    0
}

/// What the score is made of: the numbers that were really measured. `None` means "not measured",
/// which takes off nothing and is said so.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub drive: Option<crate::drive::DriveSpace>,
    /// Bytes of junk the last scan found.
    pub junk: Option<u64>,
    /// Names of the services that failed (`Some(vec![])` = looked, none failed).
    pub failed_services: Option<Vec<String>>,
}

/// Which screen the fix button opens (the sidebar order: 1 Clean up, 3 Storage, 7 Services).
pub const PAGE_CLEAN_UP: i32 = 1;
pub const PAGE_STORAGE: i32 = 3;
pub const PAGE_SERVICES: i32 = 7;

/// One thing that took points off: the points, a plain title and sentence, the exact check and rule
/// (for "what was checked"), and the fix button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub points: u32,
    pub title: String,
    pub desc: String,
    pub check: String,
    pub action: String,
    pub page: i32,
}

/// Something that was looked at and is fine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotChecked {
    pub title: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub score: u32,
    pub tag: String,
    /// Biggest loss first.
    pub findings: Vec<Finding>,
    /// Checks that passed, one plain sentence each.
    pub passed: Vec<String>,
    /// Things that are not measured (yet), and why.
    pub not_checked: Vec<NotChecked>,
}

/// The whole picture behind the score, from what was measured.
pub fn report(_inputs: &Inputs) -> Report {
    Report {
        score: 0,
        tag: String::new(),
        findings: Vec::new(),
        passed: Vec::new(),
        not_checked: Vec::new(),
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

    fn drive(used: f64) -> crate::drive::DriveSpace {
        let total = 1_000_000_000_000u64;
        crate::drive::DriveSpace {
            total,
            free: (total as f64 * (1.0 - used)) as u64,
        }
    }

    #[test]
    fn a_failed_service_takes_off_three_points_each() {
        assert_eq!(SERVICE_POINTS_EACH, 3);
        assert_eq!(service_points(0), 0);
        assert_eq!(service_points(1), 3);
        assert_eq!(service_points(4), 12);
        assert_eq!(service_points(usize::MAX), u32::MAX, "never overflows");
    }

    #[test]
    fn the_artboards_example_adds_up_to_82() {
        let r = report(&Inputs {
            drive: Some(drive(0.80)),
            junk: Some(12_600_000_000),
            failed_services: Some(vec!["fwupd-refresh.service".into()]),
        });
        assert_eq!(r.score, 100 - 8 - 5 - 3);
        assert_eq!(r.tag, "Good shape");
        let got: Vec<_> = r.findings.iter().map(|f| (f.points, f.page)).collect();
        assert_eq!(
            got,
            [(8, PAGE_STORAGE), (5, PAGE_CLEAN_UP), (3, PAGE_SERVICES)]
        );
        let d = &r.findings[0];
        assert_eq!(d.title, "Main drive is 80% full");
        assert!(d.desc.contains("200 GB free"), "{}", d.desc);
        assert!(
            d.check.contains("80.0% used")
                && d.check.contains("−2 points for every 5% above 60% used"),
            "{}",
            d.check
        );
        assert_eq!(d.action, "See what's using space");
        let j = &r.findings[1];
        assert_eq!(j.title, "12.6 GB of junk files");
        assert!(
            j.check.contains("−1 point per 2.5 GB, at most −8"),
            "{}",
            j.check
        );
        assert_eq!(j.action, "Clean up");
        let s = &r.findings[2];
        assert_eq!(s.title, "1 service failed");
        assert!(s.desc.contains("fwupd-refresh.service"), "{}", s.desc);
        assert!(
            s.check.contains("−3 points per failed service"),
            "{}",
            s.check
        );
        assert_eq!(s.action, "Open Services");
    }

    #[test]
    fn findings_are_listed_biggest_loss_first_and_services_are_plural() {
        let r = report(&Inputs {
            drive: Some(drive(0.65)),
            junk: Some(20_000_000_000),
            failed_services: Some(vec!["a.service".into(), "b.service".into()]),
        });
        let points: Vec<_> = r.findings.iter().map(|f| f.points).collect();
        assert_eq!(points, [8, 6, 2], "junk 8, services 6, drive 2");
        assert_eq!(r.findings[1].title, "2 services failed");
        assert!(
            r.findings[1].desc.contains("a.service") && r.findings[1].desc.contains("b.service")
        );
    }

    #[test]
    fn what_is_fine_is_listed_as_passed_and_costs_nothing() {
        let r = report(&Inputs {
            drive: Some(drive(0.50)),
            junk: Some(100_000_000),
            failed_services: Some(vec![]),
        });
        assert_eq!(r.score, 100);
        assert_eq!(r.tag, "Good shape");
        assert!(r.findings.is_empty());
        assert_eq!(
            r.passed,
            [
                "Your main drive has room (50% used)",
                "Junk is under 2.5 GB",
                "No service has failed",
            ]
        );
    }

    #[test]
    fn what_was_not_measured_is_said_and_takes_nothing_off() {
        let r = report(&Inputs::default());
        assert_eq!(r.score, 100);
        assert!(r.findings.is_empty() && r.passed.is_empty());
        let titles: Vec<_> = r.not_checked.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Main drive space",
                "Junk",
                "Services",
                "Waiting updates",
                "Drive health"
            ]
        );
        let why = |t: &str| {
            r.not_checked
                .iter()
                .find(|n| n.title == t)
                .unwrap()
                .why
                .clone()
        };
        assert!(
            why("Junk").contains("scan has not finished"),
            "{}",
            why("Junk")
        );
        assert!(why("Services").contains("could not be read"));
        assert!(why("Waiting updates").contains("not checked yet"));
        assert!(why("Drive health").contains("admin access"));
        // Once everything is measured only the two that are never measured remain.
        let full = report(&Inputs {
            drive: Some(drive(0.5)),
            junk: Some(0),
            failed_services: Some(vec![]),
        });
        let rest: Vec<_> = full.not_checked.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(rest, ["Waiting updates", "Drive health"]);
    }

    #[test]
    fn the_score_never_goes_below_zero_even_with_many_failures() {
        let r = report(&Inputs {
            drive: Some(drive(1.0)),
            junk: Some(u64::MAX),
            failed_services: Some((0..50).map(|i| format!("s{i}.service")).collect()),
        });
        assert_eq!(r.score, 0);
        assert_eq!(r.tag, "Needs attention");
    }

    #[test]
    fn junk_points_follow_the_artboard_rule() {
        assert_eq!(junk_points(0), 0);
        assert_eq!(junk_points(2_499_999_999), 0);
        assert_eq!(junk_points(2_500_000_000), 1);
        assert_eq!(
            junk_points(12_600_000_000),
            5,
            "the artboard: 12.6 GB is −5"
        );
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
