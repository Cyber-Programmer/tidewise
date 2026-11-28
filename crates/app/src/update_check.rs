//! "Are updates waiting?", asked before an install (WBS 6.11). The answer comes from the real lists and, when those
//! say no, from the app's own freshly refreshed private copy, so that stale real lists do not hide an update the
//! helper would then refuse over. pacman is handed in, so this is tested without it.
//!
//! Only this question uses the private copy. A plain install is still worked out against the real lists, because
//! those are what the helper installs from.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::update_model::system_block;

use tidewise_core::age::whole_days;
use tidewise_pacman::{
    InstallDecision, Lists, NotFresh, PacmanError, PendingUpdate, Resolution, UpdateLookup,
    install_decision,
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
    /// Updates waiting in the lists that answered (`IgnorePkg` ones left out).
    pub waiting: usize,
    /// How new those lists are: the real ones' newest file, or now for a fresh private copy.
    pub lists_from: Option<SystemTime>,
}

/// `pending` is `pacman -Qu` against a database folder (`None`: the real one), `refresh` brings the private copy up to
/// date, `real_newest` is the time of the newest real list.
///
/// When the real lists already say "update first" nothing is downloaded. When there is no fresh copy, the real answer
/// stands, with a note; that is not an error.
pub fn check_for_updates(
    pending: impl Fn(Option<&Path>) -> Result<Vec<PendingUpdate>, PacmanError>,
    refresh: impl FnOnce() -> Lists,
    real_newest: impl FnOnce() -> Option<SystemTime>,
    now: SystemTime,
) -> Result<UpdateCheck, PacmanError> {
    let waiting = |updates: &[PendingUpdate]| {
        matches!(
            install_decision(updates),
            InstallDecision::MustUpdateSystem { .. }
        )
    };
    let answer = |updates: &[PendingUpdate], from: Option<SystemTime>| {
        let must_update = waiting(updates);
        UpdateCheck {
            must_update,
            note: None,
            waiting: updates.iter().filter(|update| !update.ignored).count(),
            lists_from: from.filter(|_| must_update),
        }
    };
    let real = pending(None)?;
    if waiting(&real) {
        return Ok(answer(&real, real_newest()));
    }
    let why = match refresh() {
        Lists::Private(folder) => match pending(Some(&folder)) {
            Ok(updates) => return Ok(answer(&updates, Some(now))),
            Err(_) => NotFresh::Failed,
        },
        Lists::Real(why) => why,
    };
    Ok(UpdateCheck {
        must_update: false,
        waiting: 0,
        lists_from: None,
        note: Some(ListsNote {
            why,
            age_days: real_newest().map(|newest| whole_days(newest, now)),
        }),
    })
}

/// The review of "Update system" (WBS 6.5): refresh the private copy, then read it. Without a usable private copy the
/// real lists are read, with a note of their age; that is not an error.
pub fn review_updates(
    refresh: impl FnOnce() -> Lists,
    lookup: impl Fn(Option<&Path>) -> Result<UpdateLookup, PacmanError>,
    real_newest: impl FnOnce() -> Option<SystemTime>,
    now: SystemTime,
) -> Result<(UpdateLookup, Option<ListsNote>), PacmanError> {
    let why = match refresh() {
        Lists::Private(folder) => match lookup(Some(&folder)) {
            Ok(found) => return Ok((found, None)),
            Err(_) => NotFresh::Failed,
        },
        Lists::Real(why) => why,
    };
    let found = lookup(None)?;
    let note = ListsNote {
        why,
        age_days: real_newest().map(|newest| whole_days(newest, now)),
    };
    Ok((found, Some(note)))
}

/// What "Update system and install" finds out first: the system part (the update review) and the resolve of the
/// install, both against the same lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemCheck {
    pub lookup: UpdateLookup,
    pub note: Option<ListsNote>,
    /// The private copy both were read from; `None`: the real lists.
    pub dbpath: Option<PathBuf>,
    /// `None` when the system part is blocked: there is nothing to resolve.
    pub resolution: Option<Resolution>,
}

/// Refreshes the private lists, reads the update review, then resolves the install against the same folder.
pub fn prepare_system(
    refresh: impl FnOnce() -> Lists,
    lookup: impl Fn(Option<&Path>) -> Result<UpdateLookup, PacmanError>,
    resolve: impl FnOnce(Option<PathBuf>) -> Result<Resolution, PacmanError>,
    real_newest: impl FnOnce() -> Option<SystemTime>,
    now: SystemTime,
) -> Result<SystemCheck, PacmanError> {
    // The folder of the read that worked: the review falls back to the real lists by itself.
    let used = RefCell::new(None);
    let read = |folder: Option<&Path>| {
        let found = lookup(folder);
        if found.is_ok() {
            *used.borrow_mut() = folder.map(Path::to_path_buf);
        }
        found
    };
    let (found, note) = review_updates(refresh, read, real_newest, now)?;
    let dbpath = used.into_inner();
    let resolution = match system_block(&found) {
        Some(_) => None,
        None => Some(resolve(dbpath.clone())?),
    };
    Ok(SystemCheck {
        lookup: found,
        note,
        dbpath,
        resolution,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
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
        let mut answer = answer;
        // Compared on their own in `the_waiting_count_and_the_date_come_from_the_lists_that_answered`.
        answer.lists_from = None;
        (answer, refreshed.get())
    }

    fn answer(must_update: bool, why: Option<NotFresh>) -> UpdateCheck {
        UpdateCheck {
            must_update,
            waiting: usize::from(must_update),
            // `check` says the real lists are 3 days old; a private answer is from now.
            lists_from: None,
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

    fn held_lookup(name: &str) -> UpdateLookup {
        UpdateLookup::HeldBlock(tidewise_pacman::HeldBlock {
            package: name.to_string(),
            held: "x".to_string(),
        })
    }

    fn review(
        lists: Lists,
        private_fails: bool,
    ) -> Result<(UpdateLookup, Option<ListsNote>), PacmanError> {
        let newest = SystemTime::UNIX_EPOCH + Duration::from_secs(86_400 * 10);
        review_updates(
            || lists,
            |folder| match folder {
                Some(_) if private_fails => Err(PacmanError::Failed {
                    code: Some(1),
                    stderr: String::new(),
                }),
                Some(_) => Ok(held_lookup("private")),
                None => Ok(held_lookup("real")),
            },
            || Some(newest),
            newest + Duration::from_secs(86_400 * 2),
        )
    }

    #[test]
    fn the_review_reads_the_fresh_private_copy_without_a_note() {
        let lists = Lists::Private(PathBuf::from(FOLDER));
        assert_eq!(
            review(lists, false).unwrap(),
            (held_lookup("private"), None)
        );
    }

    #[test]
    fn without_a_private_copy_the_review_reads_the_real_lists_and_says_their_age() {
        let note = |why| {
            Some(ListsNote {
                why,
                age_days: Some(2),
            })
        };
        assert_eq!(
            review(Lists::Real(NotFresh::Busy), false).unwrap(),
            (held_lookup("real"), note(NotFresh::Busy))
        );
        assert_eq!(
            review(Lists::Private(PathBuf::from(FOLDER)), true).unwrap(),
            (held_lookup("real"), note(NotFresh::Failed))
        );
    }

    #[test]
    fn the_waiting_count_and_the_date_come_from_the_lists_that_answered() {
        let ask = |real: Vec<PendingUpdate>, private_updates: Vec<PendingUpdate>| {
            check_for_updates(
                |folder| {
                    Ok(if folder.is_none() {
                        real.clone()
                    } else {
                        private_updates.clone()
                    })
                },
                private,
                || days_ago(45),
                now(),
            )
            .unwrap()
        };
        // The real lists say so: their date. An ignored update is not counted.
        let got = ask(vec![update(false), update(true), update(false)], Vec::new());
        assert_eq!((got.waiting, got.lists_from), (2, days_ago(45)));
        // Only the fresh copy knows: it is from now.
        let got = ask(Vec::new(), vec![update(false)]);
        assert_eq!((got.waiting, got.lists_from), (1, Some(now())));
    }

    // ---- "update system and install": the first step ------------------------------------------------------------

    fn ready_resolution() -> Resolution {
        Resolution::Blocked(vec![tidewise_pacman::Blocker::NothingToInstall])
    }

    fn plan_lookup() -> UpdateLookup {
        UpdateLookup::Plan(Box::default())
    }

    /// Runs the step; returns it with the folder the resolve was given (`None`: not resolved).
    fn prepare(
        lists: Lists,
        found: impl Fn(Option<&Path>) -> Result<UpdateLookup, PacmanError>,
    ) -> (SystemCheck, Option<Option<PathBuf>>) {
        let resolved = RefCell::new(None);
        let got = prepare_system(
            || lists,
            found,
            |folder| {
                *resolved.borrow_mut() = Some(folder);
                Ok(ready_resolution())
            },
            || days_ago(3),
            now(),
        )
        .unwrap();
        (got, resolved.into_inner())
    }

    #[test]
    fn the_install_is_resolved_against_the_fresh_private_copy_the_review_read() {
        let (got, resolved) = prepare(private(), |_| Ok(plan_lookup()));
        assert_eq!(got.dbpath, Some(PathBuf::from(FOLDER)));
        assert_eq!(resolved, Some(Some(PathBuf::from(FOLDER))));
        assert_eq!(got.note, None);
        assert_eq!(got.resolution, Some(ready_resolution()));
    }

    #[test]
    fn without_a_private_copy_the_real_lists_are_resolved_with_the_note() {
        for (lists, lookup_fails, why) in [
            (Lists::Real(NotFresh::Busy), false, NotFresh::Busy),
            (private(), true, NotFresh::Failed),
        ] {
            let (got, resolved) = prepare(lists, |folder| match folder {
                Some(_) if lookup_fails => Err(PacmanError::TooManyRounds { limit: 1 }),
                _ => Ok(plan_lookup()),
            });
            assert_eq!((got.dbpath, resolved), (None, Some(None)));
            assert_eq!(
                got.note,
                Some(ListsNote {
                    why,
                    age_days: Some(3)
                })
            );
        }
    }

    #[test]
    fn a_blocked_system_part_is_not_resolved() {
        let held = |_: Option<&Path>| Ok(held_lookup("x"));
        let replaced = |_: Option<&Path>| {
            Ok(UpdateLookup::Plan(Box::new(tidewise_pacman::UpdatePlan {
                replacements: vec![tidewise_pacman::Replacement {
                    new: tidewise_pacman::PackageRef {
                        repo: "extra".to_string(),
                        name: "new".to_string(),
                    },
                    old: "old".to_string(),
                }],
                ..Default::default()
            })))
        };
        for (got, resolved) in [prepare(private(), held), prepare(private(), replaced)] {
            assert_eq!((got.resolution, resolved), (None, None));
        }
    }
}
