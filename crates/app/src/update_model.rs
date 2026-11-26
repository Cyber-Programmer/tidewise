//! View model for "Update system" (WBS 6.5): check, review (or up to date, or blocked), the helper run, done or failed.
//! Plain Rust, no Slint, no threads, no clock. It runs nothing: it hands out tickets and is handed the answers.

use crate::error_words::{Doing, ErrorText, error_text};
use crate::helper_request::update_request;
use crate::helper_run::JobEnd;
use crate::job::{Job, JobFailure, JobTicket};
use crate::update_check::ListsNote;
use crate::update_words::{UpdateBlock, needs_restart};
use tidewise_helper_protocol::{ErrorKind, Event, Notice, Outcome};
use tidewise_pacman::{PacmanError, RefreshWhy, UpdateLookup, UpdatePlan};

/// One check to run: refresh the private lists (`why`), then read the review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckTicket {
    pub number: u64,
    pub why: RefreshWhy,
}

/// What a check found.
pub type CheckResult = Result<(UpdateLookup, Option<ListsNote>), PacmanError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Updating {
    pub plan: UpdatePlan,
    pub job: Job,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Closed,
    Checking,
    /// Nothing the update would install. The plan may still list held packages and replacements.
    UpToDate(Box<UpdatePlan>),
    Review(Box<UpdatePlan>),
    Blocked(UpdateBlock),
    Failed {
        text: ErrorText,
    },
    /// The helper is running (or the password window is open). Nothing here can stop it.
    Updating(Box<Updating>),
    Updated {
        notices: Vec<Notice>,
        restart: bool,
    },
    UpdateFailed(JobFailure),
}

/// What the end of a job means for the screens.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ended {
    /// Lists read before are old.
    pub may_have_changed: bool,
    /// The package lists changed since the review: run this check now.
    pub check: Option<CheckTicket>,
}

#[derive(Debug, Default)]
pub struct UpdateModel {
    phase: Phase,
    last_number: u64,
    /// The check whose answer is wanted.
    wanted: Option<u64>,
    lists_note: Option<ListsNote>,
    /// The next review follows a list that changed under the user.
    lists_changed: bool,
    /// The helper refused: the next refresh may not trust the age of the private lists.
    refused: bool,
    prompt_closed: bool,
}

impl UpdateModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// Why the review was made with older lists, if it was.
    pub fn lists_note(&self) -> Option<ListsNote> {
        self.lists_note
    }

    /// The review says the lists changed since the user looked.
    pub fn lists_changed(&self) -> bool {
        self.lists_changed
    }

    pub fn prompt_was_closed(&self) -> bool {
        self.prompt_closed
    }

    /// "Check for updates" and "Check again". `None` while a check or an update is already running.
    pub fn check(&mut self) -> Option<CheckTicket> {
        if matches!(self.phase, Phase::Checking | Phase::Updating(_)) {
            return None;
        }
        let why = if self.refused {
            RefreshWhy::AfterRefusal
        } else {
            RefreshWhy::Opening
        };
        self.lists_changed = false;
        Some(self.start_check(why))
    }

    fn start_check(&mut self, why: RefreshWhy) -> CheckTicket {
        self.refused = false;
        self.prompt_closed = false;
        self.lists_note = None;
        self.last_number += 1;
        self.wanted = Some(self.last_number);
        self.phase = Phase::Checking;
        CheckTicket {
            number: self.last_number,
            why,
        }
    }

    /// The answer of check `number`; one that is no longer wanted is dropped.
    pub fn checked(&mut self, number: u64, result: CheckResult) {
        if self.wanted != Some(number) {
            return;
        }
        self.wanted = None;
        self.phase = match result {
            Ok((UpdateLookup::HeldBlock(block), note)) => {
                self.lists_note = note;
                Phase::Blocked(UpdateBlock::Held(block))
            }
            Ok((UpdateLookup::Plan(plan), note)) => {
                self.lists_note = note;
                reviewed(plan)
            }
            Err(error) => Phase::Failed {
                text: error_text(&error, Doing::CheckingUpdates),
            },
        };
    }

    /// Leaves the screen. A running job stays: there is no stop.
    pub fn cancel(&mut self) {
        if matches!(self.phase, Phase::Updating(_)) {
            return;
        }
        self.wanted = None;
        self.lists_changed = false;
        self.refused = false;
        self.phase = Phase::Closed;
    }

    /// The review's Update button: `Some` means "start the helper now".
    pub fn start_update(&mut self) -> Option<JobTicket> {
        let Phase::Review(plan) = &self.phase else {
            return None;
        };
        let plan = (**plan).clone();
        self.prompt_closed = false;
        self.lists_changed = false;
        match update_request(&plan) {
            Ok(request) => {
                self.last_number += 1;
                self.phase = Phase::Updating(Box::new(Updating {
                    plan,
                    job: Job::new(self.last_number),
                }));
                Some(JobTicket {
                    number: self.last_number,
                    request,
                })
            }
            Err(error) => {
                self.phase = Phase::UpdateFailed(JobFailure::NotSent(error));
                None
            }
        }
    }

    pub fn job_event(&mut self, number: u64, event: Event) {
        if let Phase::Updating(running) = &mut self.phase
            && running.job.number == number
        {
            running.job.event(event);
        }
    }

    /// Job `number` is over.
    pub fn job_ended(&mut self, number: u64, end: JobEnd) -> Ended {
        let Phase::Updating(running) = &self.phase else {
            return Ended::default();
        };
        if running.job.number != number {
            return Ended::default();
        }
        // A result, or none at all, means pacman may have run. The other ends are all from before it.
        let may_have_changed = matches!(end, JobEnd::Finished(_) | JobEnd::NoResult { .. });
        let mut check = None;
        match end {
            JobEnd::Finished(Outcome::Ok) => {
                let installed = running.plan.installing().map(|i| i.package.name.as_str());
                self.phase = Phase::Updated {
                    restart: needs_restart(installed),
                    notices: running.job.notices.clone(),
                };
            }
            JobEnd::PromptClosed => {
                self.prompt_closed = true;
                self.phase = Phase::Review(Box::new(running.plan.clone()));
            }
            JobEnd::Finished(Outcome::Error(ErrorKind::TransactionChanged)) => {
                self.lists_changed = true;
                check = Some(self.start_check(RefreshWhy::AfterRefusal));
            }
            end => {
                self.refused = matches!(end, JobEnd::Finished(Outcome::Error(_)));
                self.phase = Phase::UpdateFailed(JobFailure::Ended {
                    notices: running.job.notices.clone(),
                    end,
                });
            }
        }
        Ended {
            may_have_changed,
            check,
        }
    }
}

/// Where a plan leads: nothing to do, a reason it cannot go on, or the review.
fn reviewed(plan: Box<UpdatePlan>) -> Phase {
    if plan.installing().next().is_none() {
        Phase::UpToDate(plan)
    } else if !plan.conflicts.is_empty() {
        Phase::Blocked(UpdateBlock::Conflict(plan.conflicts))
    } else if !plan.picks.is_empty() {
        Phase::Blocked(UpdateBlock::Pick(plan.picks))
    } else {
        Phase::Review(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_helper_protocol::{Phase as Stage, Progress};
    use tidewise_pacman::{
        Conflict, HeldBlock, NotFresh, PackageRef, PlanItem, ProviderPick, Role,
    };

    fn pkg(name: &str) -> PackageRef {
        PackageRef {
            repo: "extra".to_string(),
            name: name.to_string(),
        }
    }

    fn item(name: &str) -> PlanItem {
        PlanItem {
            package: pkg(name),
            version: "2-1".to_string(),
            role: Role::Update,
            installed_version: Some("1-1".to_string()),
            download_size: 1,
            installed_size: 2,
            installed_size_text: String::new(),
            current_size: Some(1),
        }
    }

    fn plan(names: &[&str]) -> UpdatePlan {
        UpdatePlan {
            items: names.iter().map(|name| item(name)).collect(),
            ..UpdatePlan::default()
        }
    }

    fn found(plan: UpdatePlan) -> CheckResult {
        Ok((UpdateLookup::Plan(Box::new(plan)), None))
    }

    /// A model showing the review of `plan`.
    fn reviewing(plan: UpdatePlan) -> UpdateModel {
        let mut model = UpdateModel::new();
        let ticket = model.check().unwrap();
        model.checked(ticket.number, found(plan));
        model
    }

    /// A model whose update job is running.
    fn updating(names: &[&str]) -> (UpdateModel, u64) {
        let mut model = reviewing(plan(names));
        let job = model.start_update().unwrap();
        (model, job.number)
    }

    #[test]
    fn a_check_refreshes_as_an_opening_and_a_second_one_waits() {
        let mut model = UpdateModel::new();
        let ticket = model.check().unwrap();
        assert_eq!(ticket.why, RefreshWhy::Opening);
        assert_eq!(*model.phase(), Phase::Checking);
        assert_eq!(model.check(), None);
    }

    #[test]
    fn an_answer_to_a_cancelled_check_is_dropped() {
        let mut model = UpdateModel::new();
        let old = model.check().unwrap();
        model.cancel();
        assert_eq!(*model.phase(), Phase::Closed);
        let new = model.check().unwrap();
        model.checked(old.number, found(plan(&["zsh"])));
        assert_eq!(*model.phase(), Phase::Checking);
        model.checked(new.number, found(plan(&["zsh"])));
        assert!(matches!(model.phase(), Phase::Review(_)));
    }

    #[test]
    fn nothing_to_install_is_up_to_date_even_with_a_replacement_listed() {
        let mut replacement = plan(&["new-pkg"]);
        replacement.skipped.push(pkg("new-pkg"));
        for nothing in [plan(&[]), replacement] {
            let mut model = reviewing(nothing);
            assert!(matches!(model.phase(), Phase::UpToDate(_)));
            assert_eq!(model.start_update(), None);
        }
    }

    #[test]
    fn a_held_package_a_conflict_or_a_pick_blocks_the_update() {
        let held = HeldBlock {
            package: "a".to_string(),
            held: "b".to_string(),
        };
        let mut model = UpdateModel::new();
        let ticket = model.check().unwrap();
        model.checked(
            ticket.number,
            Ok((UpdateLookup::HeldBlock(held.clone()), None)),
        );
        assert_eq!(*model.phase(), Phase::Blocked(UpdateBlock::Held(held)));

        let conflict = Conflict {
            package: "a".to_string(),
            installed: "b".to_string(),
        };
        let pick = ProviderPick {
            dependency: "v".to_string(),
            provider: pkg("p"),
        };
        let mut with_both = plan(&["a"]);
        with_both.conflicts.push(conflict.clone());
        with_both.picks.push(pick.clone());
        let mut model = reviewing(with_both);
        assert_eq!(
            *model.phase(),
            Phase::Blocked(UpdateBlock::Conflict(vec![conflict]))
        );
        assert_eq!(model.start_update(), None);

        let mut with_pick = plan(&["a"]);
        with_pick.picks.push(pick.clone());
        assert_eq!(
            *reviewing(with_pick).phase(),
            Phase::Blocked(UpdateBlock::Pick(vec![pick]))
        );
    }

    #[test]
    fn a_failed_check_says_so_and_can_be_tried_again() {
        let mut model = UpdateModel::new();
        let ticket = model.check().unwrap();
        model.checked(
            ticket.number,
            Err(PacmanError::Failed {
                code: Some(1),
                stderr: "error: x".to_string(),
            }),
        );
        assert!(
            matches!(model.phase(), Phase::Failed { text } if text.title == "pacman couldn't check for updates")
        );
        assert!(model.check().is_some());
    }

    #[test]
    fn the_note_about_older_lists_is_kept_for_the_review() {
        let note = ListsNote {
            why: NotFresh::Failed,
            age_days: Some(3),
        };
        let mut model = UpdateModel::new();
        let ticket = model.check().unwrap();
        model.checked(
            ticket.number,
            Ok((UpdateLookup::Plan(Box::new(plan(&["zsh"]))), Some(note))),
        );
        assert_eq!(model.lists_note(), Some(note));
        assert!(!model.lists_changed());
    }

    #[test]
    fn update_starts_the_helper_once_and_nothing_can_leave_it() {
        let (mut model, number) = updating(&["zsh"]);
        assert!(matches!(model.phase(), Phase::Updating(_)));
        assert_eq!(model.start_update(), None);
        assert_eq!(model.check(), None);
        model.cancel();
        assert!(matches!(model.phase(), Phase::Updating(_)));
        model.job_event(
            number,
            Event::Progress(Progress {
                phase: Stage::Installing,
                current: 1,
                total: 1,
            }),
        );
        let Phase::Updating(running) = model.phase() else {
            panic!("{:?}", model.phase());
        };
        assert_eq!(running.job.percent, 92);
    }

    #[test]
    fn a_closed_password_window_goes_back_to_the_review() {
        let (mut model, number) = updating(&["zsh"]);
        let ended = model.job_ended(number, JobEnd::PromptClosed);
        assert!(!ended.may_have_changed);
        assert!(matches!(model.phase(), Phase::Review(_)));
        assert!(model.prompt_was_closed());
        assert!(model.start_update().is_some());
        assert!(!model.prompt_was_closed());
    }

    #[test]
    fn a_finished_update_keeps_its_notices_and_asks_for_a_restart_after_a_kernel() {
        for (names, restart) in [(&["zsh", "linux"][..], true), (&["zsh"][..], false)] {
            let (mut model, number) = updating(names);
            model.job_event(number, Event::Notice(Notice::PackagesNotInstalled(1)));
            let ended = model.job_ended(number, JobEnd::Finished(Outcome::Ok));
            assert!(ended.may_have_changed);
            assert_eq!(
                *model.phase(),
                Phase::Updated {
                    notices: vec![Notice::PackagesNotInstalled(1)],
                    restart,
                }
            );
        }
    }

    #[test]
    fn a_kernel_that_is_skipped_asks_for_no_restart() {
        let mut skipped = plan(&["zsh", "linux"]);
        skipped.skipped.push(pkg("linux"));
        let mut model = reviewing(skipped);
        let job = model.start_update().unwrap();
        model.job_ended(job.number, JobEnd::Finished(Outcome::Ok));
        assert!(matches!(
            model.phase(),
            Phase::Updated { restart: false, .. }
        ));
    }

    #[test]
    fn changed_lists_start_a_new_check_that_says_so() {
        let (mut model, number) = updating(&["zsh"]);
        let ended = model.job_ended(
            number,
            JobEnd::Finished(Outcome::Error(ErrorKind::TransactionChanged)),
        );
        let ticket = ended.check.unwrap();
        assert_eq!(ticket.why, RefreshWhy::AfterRefusal);
        assert_eq!(*model.phase(), Phase::Checking);
        model.checked(ticket.number, found(plan(&["zsh", "vim"])));
        assert!(matches!(model.phase(), Phase::Review(_)));
        assert!(model.lists_changed());
        // Only that review says it.
        let job = model.start_update().unwrap();
        model.job_ended(job.number, JobEnd::PromptClosed);
        assert!(!model.lists_changed());
    }

    #[test]
    fn after_any_other_refusal_the_next_check_refreshes_as_after_a_refusal() {
        let (mut model, number) = updating(&["zsh"]);
        let ended = model.job_ended(
            number,
            JobEnd::Finished(Outcome::Error(ErrorKind::ProviderNotChosen)),
        );
        assert_eq!(ended.check, None);
        assert!(matches!(model.phase(), Phase::UpdateFailed(_)));
        assert_eq!(model.check().unwrap().why, RefreshWhy::AfterRefusal);
        model.cancel();
        assert_eq!(model.check().unwrap().why, RefreshWhy::Opening);
    }

    #[test]
    fn events_and_ends_of_another_job_are_ignored() {
        let (mut model, number) = updating(&["zsh"]);
        let ended = model.job_ended(number + 1, JobEnd::HelperMissing);
        assert_eq!(ended, Ended::default());
        model.job_event(number + 1, Event::Notice(Notice::PackagesRemoved(1)));
        let Phase::Updating(running) = model.phase() else {
            panic!("{:?}", model.phase());
        };
        assert!(running.job.notices.is_empty());
    }
}
