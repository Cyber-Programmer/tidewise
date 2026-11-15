//! Turning pacman's non-terminal output into progress events (threat T-15; plan 5.3b, "Progress events").
//!
//! pacman prints no percentages when its output is not a terminal. What it does print, seen in the test container
//! (`tests/fixtures/helper-jobs/real-*.out`): phase headings (`resolving dependencies...`, `:: Retrieving
//! packages...`, `:: Processing package changes...`, `:: Running post-transaction hooks...`), one ` <file>
//! downloading...` line per download, and one `installing|upgrading|removing <name>...` line per package. No
//! `(1/3)` counters off a terminal. That is all the progress there is, so that is all this reports.
//!
//! Progress is advisory and never the result: a package's scriptlet can print any text, so the rules are strict. Only
//! exact line shapes count, a phase can only move forward, and a counter never goes past the confirmed total.

use tidewise_helper_protocol::{Phase, Progress};

/// Follows one pacman run. Feed it every complete line (without the new line), in order.
#[derive(Debug, Clone)]
pub struct ProgressTracker {
    total: u32,
    phase: Option<Phase>,
    current: u32,
}

impl ProgressTracker {
    /// `total` is the number of lines of the confirmed transaction; counters are clamped to it.
    pub fn new(total: u32) -> Self {
        ProgressTracker {
            total,
            phase: None,
            current: 0,
        }
    }

    /// The event this line causes, if any: a new phase, or the counter of the phase going up by one.
    pub fn line(&mut self, line: &str) -> Option<Progress> {
        if let Some(phase) = heading(line) {
            // Only forward: a scriptlet printing an old heading later cannot rewind the progress.
            if self.phase.is_none_or(|now| rank(phase) > rank(now)) {
                self.phase = Some(phase);
                self.current = 0;
                return Some(self.event(phase));
            }
            return None;
        }
        match self.phase {
            Some(Phase::Downloading) if is_download_line(line) => self.count(Phase::Downloading),
            Some(Phase::Installing) if is_package_line(line) => self.count(Phase::Installing),
            _ => None,
        }
    }

    /// One more done in `phase`, unless the confirmed total is already reached.
    fn count(&mut self, phase: Phase) -> Option<Progress> {
        if self.current >= self.total {
            return None;
        }
        self.current += 1;
        Some(self.event(phase))
    }

    fn event(&self, phase: Phase) -> Progress {
        Progress {
            phase,
            current: self.current,
            total: self.total,
        }
    }
}

fn rank(phase: Phase) -> u8 {
    match phase {
        Phase::Checking => 1,
        Phase::Downloading => 2,
        Phase::Installing => 3,
        Phase::Hooks => 4,
    }
}

/// The phase a whole-line heading starts. Exact lines only (as printed in the container).
fn heading(line: &str) -> Option<Phase> {
    match line {
        ":: Synchronizing package databases..."
        | ":: Starting full system upgrade..."
        | "resolving dependencies..." => Some(Phase::Checking),
        ":: Retrieving packages..." => Some(Phase::Downloading),
        ":: Processing package changes..." => Some(Phase::Installing),
        ":: Running post-transaction hooks..." => Some(Phase::Hooks),
        _ => None,
    }
}

/// A word with no space or other white space in it, and not empty.
fn is_word(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_whitespace)
}

/// ` <file> downloading...`
fn is_download_line(line: &str) -> bool {
    line.strip_prefix(' ')
        .and_then(|rest| rest.strip_suffix(" downloading..."))
        .is_some_and(is_word)
}

/// `installing <name>...` and the other verbs pacman prints for one package.
fn is_package_line(line: &str) -> bool {
    [
        "installing ",
        "upgrading ",
        "downgrading ",
        "reinstalling ",
        "removing ",
    ]
    .iter()
    .any(|verb| {
        line.strip_prefix(verb)
            .and_then(|rest| rest.strip_suffix("..."))
            .is_some_and(is_word)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTALL_TWO: &str =
        include_str!("../../../tests/fixtures/helper-jobs/real-install-two.out");
    const INSTALL_SCRIPTLET: &str =
        include_str!("../../../tests/fixtures/helper-jobs/real-install-scriptlet.out");
    const UPDATE_DECLINED: &str =
        include_str!("../../../tests/fixtures/helper-jobs/real-update-declined.out");
    const SYNC_OK: &str = include_str!("../../../tests/fixtures/helper-jobs/sync-ok.out");
    const CONFLICT: &str = include_str!("../../../tests/fixtures/helper-jobs/conflict.out");

    fn events(total: u32, text: &str) -> Vec<(Phase, u32, u32)> {
        let mut tracker = ProgressTracker::new(total);
        text.split('\n')
            .filter_map(|line| tracker.line(line))
            .map(|p| (p.phase, p.current, p.total))
            .collect()
    }

    // ---- real output --------------------------------------------------------------------------------------

    #[test]
    fn a_real_install_of_two_packages_goes_through_the_four_phases() {
        assert_eq!(
            events(2, INSTALL_TWO),
            vec![
                (Phase::Checking, 0, 2),
                (Phase::Downloading, 0, 2),
                (Phase::Downloading, 1, 2),
                (Phase::Downloading, 2, 2),
                (Phase::Installing, 0, 2),
                (Phase::Installing, 1, 2),
                (Phase::Installing, 2, 2),
                (Phase::Hooks, 0, 2),
            ]
        );
    }

    #[test]
    fn a_real_install_with_a_scriptlet_ignores_the_scriptlets_own_words() {
        let seen = events(1, INSTALL_SCRIPTLET);
        assert_eq!(
            seen,
            vec![
                (Phase::Checking, 0, 1),
                (Phase::Downloading, 0, 1),
                (Phase::Downloading, 1, 1),
                (Phase::Installing, 0, 1),
                (Phase::Installing, 1, 1),
                (Phase::Hooks, 0, 1),
            ]
        );
    }

    #[test]
    fn a_real_update_starts_at_its_own_heading_and_counts_an_upgrade() {
        // The replacement was declined, so only one package was downloaded and upgraded of the two confirmed.
        assert_eq!(
            events(2, UPDATE_DECLINED),
            vec![
                (Phase::Checking, 0, 2),
                (Phase::Downloading, 0, 2),
                (Phase::Downloading, 1, 2),
                (Phase::Installing, 0, 2),
                (Phase::Installing, 1, 2),
                (Phase::Hooks, 0, 2),
            ]
        );
    }

    #[test]
    fn a_sync_is_checking_and_its_downloads_are_not_counted() {
        assert_eq!(events(5, SYNC_OK), vec![(Phase::Checking, 0, 5)]);
    }

    #[test]
    fn an_aborted_transaction_only_got_as_far_as_checking() {
        assert_eq!(events(1, CONFLICT), vec![(Phase::Checking, 0, 1)]);
    }

    // ---- hostile or odd text ------------------------------------------------------------------------------

    fn run(total: u32, lines: &[&str]) -> Vec<(Phase, u32, u32)> {
        let mut tracker = ProgressTracker::new(total);
        lines
            .iter()
            .filter_map(|line| tracker.line(line))
            .map(|p| (p.phase, p.current, p.total))
            .collect()
    }

    #[test]
    fn a_phase_never_moves_backwards() {
        let seen = run(
            3,
            &[
                ":: Processing package changes...",
                ":: Retrieving packages...",
                "resolving dependencies...",
                ":: Processing package changes...",
                ":: Running post-transaction hooks...",
                ":: Processing package changes...",
            ],
        );
        assert_eq!(seen, vec![(Phase::Installing, 0, 3), (Phase::Hooks, 0, 3)]);
    }

    #[test]
    fn counters_stop_at_the_confirmed_total() {
        let mut lines = vec![":: Processing package changes..."];
        lines.extend(std::iter::repeat_n("installing x...", 50));
        let seen = run(2, &lines);
        assert_eq!(
            seen,
            vec![
                (Phase::Installing, 0, 2),
                (Phase::Installing, 1, 2),
                (Phase::Installing, 2, 2)
            ]
        );
    }

    #[test]
    fn a_total_of_zero_never_counts() {
        let seen = run(0, &[":: Processing package changes...", "installing x..."]);
        assert_eq!(seen, vec![(Phase::Installing, 0, 0)]);
    }

    #[test]
    fn counting_lines_only_count_in_their_own_phase() {
        let seen = run(
            3,
            &[
                "installing early...",
                " early downloading...",
                "resolving dependencies...",
                "installing too-early...",
                ":: Retrieving packages...",
                "installing still-early...",
                " one downloading...",
                ":: Processing package changes...",
                " late downloading...",
                "upgrading one...",
            ],
        );
        assert_eq!(
            seen,
            vec![
                (Phase::Checking, 0, 3),
                (Phase::Downloading, 0, 3),
                (Phase::Downloading, 1, 3),
                (Phase::Installing, 0, 3),
                (Phase::Installing, 1, 3),
            ]
        );
    }

    #[test]
    fn only_exact_line_shapes_count() {
        let seen = run(
            9,
            &[
                ":: Processing package changes...",
                "installing a b...",
                "installing ...",
                "installing x",
                "Installing x...",
                " installing x...",
                "installing x...\r",
                "installing  x...",
                "xinstalling x...",
                "reinstalling x...",
                "downgrading y...",
                "removing z...",
                "",
                "   ",
                &"i".repeat(100_000),
            ],
        );
        assert_eq!(
            seen,
            vec![
                (Phase::Installing, 0, 9),
                (Phase::Installing, 1, 9),
                (Phase::Installing, 2, 9),
                (Phase::Installing, 3, 9)
            ]
        );
    }

    #[test]
    fn a_heading_must_be_the_whole_line() {
        let seen = run(
            1,
            &[
                ":: Retrieving packages... extra",
                " :: Retrieving packages...",
                ":: retrieving packages...",
                ":: Running post-transaction hooks....",
            ],
        );
        assert_eq!(seen, vec![]);
    }

    #[test]
    fn an_event_carries_only_numbers_and_a_phase() {
        let mut tracker = ProgressTracker::new(1);
        let event = tracker.line("resolving dependencies...").unwrap();
        assert_eq!(event.phase, Phase::Checking);
        assert_eq!((event.current, event.total), (0, 1));
    }
}
