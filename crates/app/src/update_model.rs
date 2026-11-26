//! View model for "Update system" (WBS 6.5): check, review (or up to date, or blocked), the helper run, done or failed.
//! Plain Rust, no Slint, no threads, no clock. It runs nothing: it hands out tickets and is handed the answers.

use crate::error_words::ErrorText;
use crate::helper_run::JobEnd;
use crate::job::{Job, JobFailure, JobTicket};
use crate::update_check::ListsNote;
use crate::update_words::UpdateBlock;
use tidewise_helper_protocol::{Event, Notice};
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
        None
    }

    /// The review says the lists changed since the user looked.
    pub fn lists_changed(&self) -> bool {
        false
    }

    pub fn prompt_was_closed(&self) -> bool {
        false
    }

    /// "Check for updates" and "Check again". `None` while a check or an update is already running.
    pub fn check(&mut self) -> Option<CheckTicket> {
        None
    }

    pub fn checked(&mut self, _number: u64, _result: CheckResult) {}

    pub fn cancel(&mut self) {}

    /// The review's Update button: `Some` means "start the helper now".
    pub fn start_update(&mut self) -> Option<JobTicket> {
        None
    }

    pub fn job_event(&mut self, _number: u64, _event: Event) {}

    pub fn job_ended(&mut self, _number: u64, _end: JobEnd) -> Ended {
        Ended::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_check::ListsNote;
    use tidewise_helper_protocol::{ErrorKind, Notice, Outcome, Phase as Stage, Progress};
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
