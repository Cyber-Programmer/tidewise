//! "Are updates waiting?", asked before an install (WBS 6.11). The answer comes from the real lists and, when those
//! say no, from the app's own freshly refreshed private copy, so that stale real lists do not hide an update the
//! helper would then refuse over. pacman is handed in, so this is tested without it.
//!
//! Only this question uses the private copy. A plain install is still worked out against the real lists, because
//! those are what the helper installs from.

use std::path::Path;
use std::time::SystemTime;

use tidewise_core::age::whole_days;
use tidewise_pacman::{
    InstallDecision, Lists, NotFresh, PacmanError, PendingUpdate, install_decision,
};

/// Why the answer comes from the real lists only, and how old those are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListsNote {
    pub why: NotFresh,
    /// Whole days since the newest real list, or `None` when that is not known.
    pub age_days: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateCheck {
    pub must_update: bool,
    pub note: Option<ListsNote>,
}

/// `pending` is `pacman -Qu` against a database folder (`None`: the real one), `refresh` brings the private copy up to
/// date, `real_newest` is the time of the newest real list.
pub fn check_for_updates(
    _pending: impl Fn(Option<&Path>) -> Result<Vec<PendingUpdate>, PacmanError>,
    _refresh: impl FnOnce() -> Lists,
    _real_newest: impl FnOnce() -> Option<SystemTime>,
    _now: SystemTime,
) -> Result<UpdateCheck, PacmanError> {
    Ok(UpdateCheck {
        must_update: false,
        note: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::time::Duration;

    const FOLDER: &str = "/run/user/1000/tidewise/lists/";

    fn update(ignored: bool) -> PendingUpdate {
        PendingUpdate {
            name: "zsh".to_string(),
            installed_version: "5.9-1".to_string(),
            new_version: "5.9-2".to_string(),
            ignored,
        }
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(100 * 86_400)
    }

    fn days_ago(days: u64) -> Option<SystemTime> {
        Some(now() - Duration::from_secs(days * 86_400))
    }

    /// Runs the check with these answers; also says whether a refresh was asked for.
    fn check(
        real: Vec<PendingUpdate>,
        lists: Lists,
        private: Result<Vec<PendingUpdate>, PacmanError>,
    ) -> (UpdateCheck, bool) {
        let refreshed = Cell::new(false);
        let answer = check_for_updates(
            |folder| match folder {
                None => Ok(real.clone()),
                Some(path) => {
                    assert_eq!(path, Path::new(FOLDER));
                    match &private {
                        Ok(updates) => Ok(updates.clone()),
                        Err(_) => Err(PacmanError::TooManyRounds { limit: 1 }),
                    }
                }
            },
            || {
                refreshed.set(true);
                lists
            },
            || days_ago(3),
            now(),
        )
        .unwrap();
        (answer, refreshed.get())
    }

    fn answer(must_update: bool, why: Option<NotFresh>) -> UpdateCheck {
        UpdateCheck {
            must_update,
            note: why.map(|why| ListsNote {
                why,
                age_days: Some(3),
            }),
        }
    }

    fn private() -> Lists {
        Lists::Private(PathBuf::from(FOLDER))
    }

    #[test]
    fn updates_in_the_real_lists_answer_at_once_without_any_download() {
        let got = check(vec![update(false)], private(), Ok(Vec::new()));
        assert_eq!(got, (answer(true, None), false));
    }

    #[test]
    fn an_update_only_the_fresh_lists_know_about_means_update_first() {
        let got = check(Vec::new(), private(), Ok(vec![update(false)]));
        assert_eq!(got, (answer(true, None), true));
    }

    #[test]
    fn nothing_waiting_in_either_means_carry_on() {
        let got = check(Vec::new(), private(), Ok(Vec::new()));
        assert_eq!(got, (answer(false, None), true));
    }

    #[test]
    fn an_ignored_update_does_not_count_in_the_fresh_lists_either() {
        let got = check(Vec::new(), private(), Ok(vec![update(true)]));
        assert_eq!(got, (answer(false, None), true));
    }

    #[test]
    fn without_a_fresh_copy_the_real_answer_stands_and_says_why_and_how_old() {
        for why in [NotFresh::Unavailable, NotFresh::Busy, NotFresh::Failed] {
            let got = check(Vec::new(), Lists::Real(why), Ok(vec![update(false)]));
            assert_eq!(got, (answer(false, Some(why)), true), "{why:?}");
        }
    }

    #[test]
    fn a_private_copy_that_cannot_be_read_is_a_failed_refresh_not_an_error() {
        let got = check(
            Vec::new(),
            private(),
            Err(PacmanError::TooManyRounds { limit: 1 }),
        );
        assert_eq!(got, (answer(false, Some(NotFresh::Failed)), true));
    }

    #[test]
    fn real_lists_with_no_known_time_have_no_age() {
        let got = check_for_updates(
            |_| Ok(Vec::new()),
            || Lists::Real(NotFresh::Unavailable),
            || None,
            now(),
        )
        .unwrap();
        assert_eq!(got.note.map(|note| note.age_days), Some(None));
    }

    #[test]
    fn an_error_from_the_real_lists_is_still_an_error() {
        let got = check_for_updates(
            |_| Err(PacmanError::TooManyRounds { limit: 1 }),
            || panic!("no refresh when pacman itself fails"),
            || None,
            now(),
        );
        assert!(got.is_err());
    }
}
