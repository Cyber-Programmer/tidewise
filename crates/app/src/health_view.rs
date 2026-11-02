//! The "Why your score is N" page: the sum, each thing that lowered the score with the exact check
//! behind it, the checks that passed and what is not measured. Plain data from
//! `tidewise_core::health::Report`; `view.rs` copies it into Slint.

use tidewise_core::health::{Report, PAGE_SERVICES};

/// 0 amber (it costs points), 1 red (a failure), 2 quiet.
pub type Tone = i32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// `−8`
    pub points: String,
    pub tone: Tone,
    pub title: String,
    pub desc: String,
    /// The exact check and rule, shown when the item is open.
    pub check: String,
    /// The fix button.
    pub action: String,
    /// The sidebar page it opens.
    pub page: i32,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sum {
    /// `100`, `− 8`, `− 5`, `=`, `82`
    pub text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub score: u32,
    pub tag: String,
    /// `Why your score is 82`
    pub title: String,
    pub sum: Vec<Sum>,
    pub issues: Vec<Issue>,
    pub passed: Vec<String>,
    /// Title and why, for "Not measured".
    pub not_checked: Vec<(String, String)>,
}

/// Build the page. `open` lists the issues (by position) that are expanded.
pub fn screen(_report: &Report, _open: &[usize]) -> Screen {
    Screen {
        score: 0,
        tag: String::new(),
        title: String::new(),
        sum: Vec::new(),
        issues: Vec::new(),
        passed: Vec::new(),
        not_checked: Vec::new(),
    }
}

#[allow(dead_code)]
fn unused() -> i32 {
    PAGE_SERVICES
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_core::health::{report, Inputs};

    fn artboard() -> Report {
        report(&Inputs {
            drive: Some(tidewise_core::drive::DriveSpace {
                total: 1_000_000_000_000,
                free: 200_000_000_000,
            }),
            junk: Some(12_600_000_000),
            failed_services: Some(vec!["fwupd-refresh.service".into()]),
        })
    }

    #[test]
    fn the_title_and_the_sum_spell_out_the_score() {
        let s = screen(&artboard(), &[]);
        assert_eq!((s.score, s.tag.as_str()), (84, "Good shape"));
        assert_eq!(s.title, "Why your score is 84");
        let text: Vec<_> = s.sum.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(text, ["100", "− 8", "− 5", "− 3", "=", "84"]);
        assert_eq!(s.sum[1].tone, 0, "disk costs points");
        assert_eq!(s.sum[3].tone, 1, "a failed service is a failure");
        assert_eq!(s.sum[5].tone, 2);
    }

    #[test]
    fn each_issue_has_points_words_the_check_and_a_fix_page() {
        let s = screen(&artboard(), &[]);
        assert_eq!(s.issues.len(), 3);
        let d = &s.issues[0];
        assert_eq!((d.points.as_str(), d.tone), ("−8", 0));
        assert_eq!(d.title, "Main drive is 80% full");
        assert!(d
            .check
            .contains("rule: −2 points for every 5% above 60% used"));
        assert_eq!((d.action.as_str(), d.page), ("See what's using space", 3));
        let svc = &s.issues[2];
        assert_eq!((svc.points.as_str(), svc.tone, svc.page), ("−3", 1, 7));
        assert_eq!(svc.action, "Open Services");
    }

    #[test]
    fn only_the_opened_issues_are_expanded() {
        let s = screen(&artboard(), &[1]);
        let open: Vec<_> = s.issues.iter().map(|i| i.open).collect();
        assert_eq!(open, [false, true, false]);
        assert!(
            screen(&artboard(), &[9]).issues.iter().all(|i| !i.open),
            "unknown positions are ignored"
        );
    }

    #[test]
    fn a_healthy_pc_has_no_issues_and_lists_what_passed_and_what_is_not_measured() {
        let r = report(&Inputs {
            drive: Some(tidewise_core::drive::DriveSpace {
                total: 100,
                free: 60,
            }),
            junk: Some(0),
            failed_services: Some(vec![]),
        });
        let s = screen(&r, &[]);
        assert_eq!((s.score, s.title.as_str()), (100, "Why your score is 100"));
        assert!(s.issues.is_empty());
        let text: Vec<_> = s.sum.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(text, ["100", "=", "100"], "nothing was taken off");
        assert_eq!(s.passed.len(), 3);
        let titles: Vec<_> = s.not_checked.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(titles, ["Waiting updates", "Drive health"]);
    }
}
