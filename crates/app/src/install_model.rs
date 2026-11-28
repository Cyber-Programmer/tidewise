//! View model for the install screens (WBS 6.2): what is being installed, the questions the resolver asked, the
//! answers so far, and which resolve is running. Plain Rust, no Slint, no threads, no clock. It never runs pacman;
//! it hands out `Ticket`s and is handed the answers.
//!
//! One resolve runs at a time, like the search model: a ticket asked for while another runs waits, a newer waiting
//! ticket replaces an older one, and an answer is used only if its number is the one wanted.

use crate::error_words::{Doing, ErrorText, error_text};
use crate::helper_request::install_request;
use crate::helper_run::JobEnd;
use crate::job::{Job, JobFailure, JobTicket};
use crate::update_check::{ListsNote, SystemCheck};
use crate::update_model::system_block;
use crate::update_words::{UpdateBlock, needs_restart};
use std::path::PathBuf;
use std::time::SystemTime;
use tidewise_helper_protocol::{ErrorKind, Event, Notice, Outcome};
use tidewise_pacman::{
    Blocker, GroupAnswer, InstallDecision, InstallPlan, PackageRef, PacmanError, ProviderAnswer,
    Question, RefreshWhy, Resolution, ResolveInput, UpdateLookup, UpdatePlan,
};

/// What the user asked to install (from Search). The answers are kept apart, in `Step`s.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    pub targets: Vec<PackageRef>,
    pub groups: Vec<String>,
}

/// One resolve to run: the number says which wish the answer belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub number: u64,
    pub input: ResolveInput,
    /// The first resolve of an install first asks whether the system needs updating (`pacman -Qu`).
    pub check_lists: bool,
    /// "Update system and install", first step: refresh the private lists (this way), read the system update and
    /// resolve against it. `None` for every other ticket.
    pub refresh_system: Option<RefreshWhy>,
}

/// What the stale card says is waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Waiting {
    pub count: usize,
    /// How new the lists behind that count are, when known.
    pub lists_from: Option<SystemTime>,
}

/// The system part of "update system and install".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemPart {
    pub plan: UpdatePlan,
    /// The private copy everything was read from; `None`: the real lists.
    pub dbpath: Option<PathBuf>,
}

/// What a worker found out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkAnswer {
    /// `pacman -Qu` says updates are waiting: the resolver was not run.
    MustUpdate(Waiting),
    /// The first step of "update system and install".
    System(Box<SystemCheck>),
    Resolved(Resolution),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Closed,
    /// A resolve is wanted.
    Working,
    /// A question is shown (`steps[current]`).
    Asking,
    /// The system must be updated first.
    Stale,
    /// "Update system and install": the system update cannot go ahead.
    SystemBlocked(UpdateBlock),
    Blocked(Vec<Blocker>),
    Failed {
        text: ErrorText,
        can_retry: bool,
    },
    /// Every question is answered; 6.3 shows the review.
    Ready(Box<InstallPlan>),
    /// The helper is running (or the password window is open). Nothing here can stop it.
    Installing(Box<Installing>),
    /// The helper said it worked.
    Installed {
        notices: Vec<Notice>,
    },
    InstallFailed(JobFailure),
}

/// The running install: its plan (for the way back when the password window is closed) and the helper run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installing {
    pub plan: InstallPlan,
    pub job: Job,
}

/// The user's answer to one question, changed while it is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Draft {
    /// The chosen candidate.
    Provider(usize),
    /// One flag per group member, in the question's order. Installed members are never ticked.
    Members(Vec<bool>),
    /// Installed, same version: leave it or reinstall.
    Installed { reinstall: bool },
    /// Installed, another version than the repository's: it is left out; there is nothing to choose.
    Differs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The resolve call that asked it (`ResolveInput::round`).
    pub round: u32,
    pub question: Question,
    pub draft: Draft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Back {
    /// Still on the install screen.
    Stayed,
    /// Back to Search.
    Closed,
}

#[derive(Debug, Default)]
pub struct InstallModel {
    phase: Phase,
    request: Request,
    steps: Vec<Step>,
    current: usize,
    last_number: u64,
    /// How many packages the confirmed list of the last run had.
    listed: usize,
    wanted: Option<u64>,
    running: Option<Ticket>,
    pending: Option<Ticket>,
    slow: bool,
    focus_serial: u32,
    lists_note: Option<ListsNote>,
    /// The review says that the password window was closed.
    prompt_closed: bool,
    /// The review a failure card can go back to: only after a refusal whose text says "Go back".
    way_back: Option<Box<InstallPlan>>,
    /// "Update system and install" was chosen.
    updating: bool,
    /// Its system part, once the first step has answered.
    system: Option<SystemPart>,
    /// How the first step refreshes the private lists (`None`: as when opening).
    refresh_why: Option<RefreshWhy>,
    waiting: Option<Waiting>,
    lists_changed: bool,
    system_rows_open: bool,
    restart_wanted: bool,
    follow_up: Option<Ticket>,
}

impl Draft {
    /// The answer a question starts with: pacman's own choice, every missing member, "leave it".
    fn first_for(question: &Question) -> Self {
        match question {
            Question::ChooseProvider { default, .. } => Self::Provider(*default),
            Question::ChooseGroupMembers { members, .. } => Self::Members(
                members
                    .iter()
                    .map(|member| member.installed_version.is_none())
                    .collect(),
            ),
            Question::AlreadyInstalled {
                differs_without_update: true,
                ..
            } => Self::Differs,
            Question::AlreadyInstalled { .. } => Self::Installed { reinstall: false },
        }
    }
}

impl InstallModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn is_open(&self) -> bool {
        self.phase != Phase::Closed
    }

    pub fn request(&self) -> &Request {
        &self.request
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn is_slow(&self) -> bool {
        self.slow
    }

    /// Changes whenever a new card is shown, so the window can move the focus to it.
    pub fn focus_serial(&self) -> u32 {
        self.focus_serial
    }

    fn refocus(&mut self) {
        self.focus_serial = self.focus_serial.wrapping_add(1);
    }

    /// The input for the next resolve: the request plus every answer given so far.
    pub fn input(&self) -> ResolveInput {
        let mut input = ResolveInput {
            targets: self.request.targets.clone(),
            groups: self.request.groups.clone(),
            // Update and install is resolved against the lists its system part was read from.
            dbpath: self.system.as_ref().and_then(|part| part.dbpath.clone()),
            ..ResolveInput::default()
        };
        for step in &self.steps {
            match (&step.question, &step.draft) {
                (
                    Question::ChooseProvider {
                        dependency,
                        candidates,
                        ..
                    },
                    Draft::Provider(index),
                ) => {
                    if let Some(candidate) = candidates.get(*index) {
                        input.provider_answers.push(ProviderAnswer {
                            dependency: dependency.clone(),
                            chosen: candidate.package.clone(),
                        });
                    }
                }
                (Question::ChooseGroupMembers { group, members }, Draft::Members(ticked)) => {
                    input.group_answers.push(GroupAnswer {
                        group: group.clone(),
                        members: members
                            .iter()
                            .zip(ticked)
                            .filter(|(_, on)| **on)
                            .map(|(member, _)| member.package.clone())
                            .collect(),
                    });
                }
                (
                    Question::AlreadyInstalled { target, .. },
                    Draft::Installed { reinstall: true },
                ) => input.reinstall.push(target.clone()),
                (Question::AlreadyInstalled { target, .. }, _) => {
                    input.targets.retain(|other| other != target);
                }
                _ => {}
            }
        }
        // The first resolve is round 0; after answers, the round after the newest one asked.
        input.round = self
            .steps
            .iter()
            .map(|s| s.round)
            .max()
            .map_or(0, |r| r + 1);
        input
    }

    /// Wants a resolve of `input()`. One runs at a time: with one running, the new ticket waits (and replaces
    /// an older waiting one).
    fn start(&mut self, check_lists: bool) -> Option<Ticket> {
        self.last_number += 1;
        let ticket = Ticket {
            number: self.last_number,
            input: self.input(),
            check_lists,
            refresh_system: (self.updating && self.system.is_none())
                .then(|| self.refresh_why.unwrap_or(RefreshWhy::Opening)),
        };
        self.wanted = Some(ticket.number);
        self.slow = false;
        self.phase = Phase::Working;
        self.refocus();
        if self.running.is_some() {
            self.pending = Some(ticket);
            None
        } else {
            self.running = Some(ticket.clone());
            Some(ticket)
        }
    }

    fn begin(&mut self, request: Request, check_lists: bool, updating: bool) -> Option<Ticket> {
        if self.is_installing() {
            return None;
        }
        self.forget_the_update();
        self.updating = updating;
        self.prompt_closed = false;
        self.way_back = None;
        self.request = request;
        self.steps.clear();
        self.current = 0;
        self.lists_note = None;
        self.start(check_lists)
    }

    /// Drops everything "update system and install" holds.
    fn forget_the_update(&mut self) {
        self.updating = false;
        self.system = None;
        self.refresh_why = None;
        self.waiting = None;
        self.lists_changed = false;
        self.system_rows_open = false;
        self.restart_wanted = false;
        self.follow_up = None;
    }

    /// Install this package. `Some(ticket)`: start the worker now; `None`: it waits for the running one.
    pub fn open(&mut self, target: PackageRef) -> Option<Ticket> {
        self.begin(
            Request {
                targets: vec![target],
                groups: Vec::new(),
            },
            true,
            false,
        )
    }

    /// Install a whole group (reached only through the debug entry until WBS 6.12).
    pub fn open_group(&mut self, group: &str) -> Option<Ticket> {
        self.begin(
            Request {
                targets: Vec::new(),
                groups: vec![group.to_string()],
            },
            true,
            false,
        )
    }

    /// Replaces the answer of the question shown. An earlier round's answer that really changes makes the
    /// later rounds out of date: their questions and answers go, and the resolver asks again.
    fn change(&mut self, draft: Draft) {
        if self.phase != Phase::Asking {
            return;
        }
        let Some(step) = self.steps.get_mut(self.current) else {
            return;
        };
        if step.draft == draft {
            return;
        }
        step.draft = draft;
        let round = step.round;
        self.steps.retain(|other| other.round <= round);
    }

    /// Picks candidate `index` of the provider question shown, or the option of an installed-package question
    /// (0 = leave it, 1 = reinstall).
    pub fn choose(&mut self, index: usize) {
        let Some(step) = self.steps.get(self.current) else {
            return;
        };
        let draft = match &step.question {
            Question::ChooseProvider { candidates, .. } if index < candidates.len() => {
                Draft::Provider(index)
            }
            Question::AlreadyInstalled {
                differs_without_update: false,
                ..
            } if index <= 1 => Draft::Installed {
                reinstall: index == 1,
            },
            _ => return,
        };
        self.change(draft);
    }

    /// Flips one member of the group question shown. Installed members cannot be ticked.
    pub fn toggle(&mut self, index: usize) {
        let Some(Step {
            question: Question::ChooseGroupMembers { members, .. },
            draft: Draft::Members(ticked),
            ..
        }) = self.steps.get(self.current)
        else {
            return;
        };
        if members
            .get(index)
            .is_none_or(|member| member.installed_version.is_some())
        {
            return;
        }
        let mut ticked = ticked.clone();
        if let Some(flag) = ticked.get_mut(index) {
            *flag = !*flag;
        }
        self.change(Draft::Members(ticked));
    }

    fn set_all_members(&mut self, on: bool) {
        let Some(Step {
            question: Question::ChooseGroupMembers { members, .. },
            ..
        }) = self.steps.get(self.current)
        else {
            return;
        };
        let ticked = members
            .iter()
            .map(|member| on && member.installed_version.is_none())
            .collect();
        self.change(Draft::Members(ticked));
    }

    pub fn select_all(&mut self) {
        self.set_all_members(true);
    }

    pub fn select_none(&mut self) {
        self.set_all_members(false);
    }

    /// Can the question shown be left with its answer? No members ticked, or the only package asked for left
    /// out, give nothing to go on with.
    pub fn can_go_on(&self) -> bool {
        let Some(step) = self.steps.get(self.current) else {
            return false;
        };
        match (&step.draft, &step.question) {
            (Draft::Members(ticked), _) => ticked.iter().any(|on| *on),
            (Draft::Differs, Question::AlreadyInstalled { target, .. }) => {
                // Leaving out the one package asked for would leave nothing to install.
                !(self.request.groups.is_empty()
                    && self.request.targets.as_slice() == std::slice::from_ref(target))
            }
            _ => true,
        }
    }

    /// The primary button of a question: the next question, or after the last one a new resolve.
    pub fn go_on(&mut self) -> Option<Ticket> {
        if self.phase != Phase::Asking || !self.can_go_on() {
            return None;
        }
        if self.current + 1 < self.steps.len() {
            self.current += 1;
            self.refocus();
            return None;
        }
        self.start(false)
    }

    /// Stops wanting a resolve. A worker that is running finishes unseen.
    fn drop_wanted(&mut self) {
        self.wanted = None;
        self.pending = None;
        self.slow = false;
    }

    /// The secondary button: one question back, or out of the install screen. A wanted resolve is dropped.
    pub fn back(&mut self) -> Back {
        self.prompt_closed = false;
        match self.phase {
            Phase::Closed => Back::Closed,
            Phase::Installing(_) => Back::Stayed,
            // "Back" to the review after the one refusal that asks for it; otherwise "Done" and "Close".
            Phase::Installed { .. } | Phase::InstallFailed(_) => match self.way_back.take() {
                Some(plan) => {
                    self.phase = Phase::Ready(plan);
                    self.refocus();
                    Back::Stayed
                }
                None => {
                    self.cancel();
                    Back::Closed
                }
            },
            Phase::Asking if self.current > 0 => {
                self.current -= 1;
                self.refocus();
                Back::Stayed
            }
            Phase::Asking => {
                self.cancel();
                Back::Closed
            }
            _ if self.steps.is_empty() => {
                self.cancel();
                Back::Closed
            }
            _ => {
                self.drop_wanted();
                self.current = self.steps.len() - 1;
                self.phase = Phase::Asking;
                self.refocus();
                Back::Stayed
            }
        }
    }

    /// Leaves the install screen, whatever it shows. A wanted resolve is dropped.
    pub fn cancel(&mut self) {
        if self.is_installing() {
            return;
        }
        self.forget_the_update();
        self.prompt_closed = false;
        self.way_back = None;
        self.drop_wanted();
        self.phase = Phase::Closed;
        self.request = Request::default();
        self.steps.clear();
        self.current = 0;
        self.refocus();
    }

    /// "Install cronie instead": starts again with the replacement.
    pub fn install_instead(&mut self) -> Option<Ticket> {
        let Phase::Blocked(blockers) = &self.phase else {
            return None;
        };
        let [Blocker::ReplacedBy { requested, by }] = blockers.as_slice() else {
            return None;
        };
        let [replacement] = by.as_slice() else {
            return None;
        };
        let mut request = self.request.clone();
        for target in &mut request.targets {
            if target == requested {
                target.clone_from(replacement);
            }
        }
        self.begin(request, false, self.updating)
    }

    /// A button of the shared-library card: a normal install of that candidate, from the start.
    pub fn install_choice(&mut self, index: usize) -> Option<Ticket> {
        let Phase::Blocked(blockers) = &self.phase else {
            return None;
        };
        let Some(Blocker::SonameChoice { candidates, .. }) = blockers.first() else {
            return None;
        };
        let candidate = candidates.get(index)?.clone();
        // Inside "update system and install" the candidate is installed the same way: one path, no stale card again.
        if self.updating {
            let request = Request {
                targets: vec![candidate],
                groups: Vec::new(),
            };
            self.begin(request, false, true)
        } else {
            self.open(candidate)
        }
    }

    /// "Try again" after a failed resolve, and "Check again" after a failed install: that one starts the request
    /// afresh, because what failed may have changed what there is to ask.
    pub fn try_again(&mut self) -> Option<Ticket> {
        if matches!(self.phase, Phase::InstallFailed(_)) {
            let request = self.request.clone();
            return self.begin(request, true, false);
        }
        // The system part is read again: whatever blocks it may be gone.
        if matches!(self.phase, Phase::SystemBlocked(_)) {
            self.system = None;
            return self.start(false);
        }
        if matches!(
            self.phase,
            Phase::Failed {
                can_retry: true,
                ..
            }
        ) {
            self.start(false)
        } else {
            None
        }
    }

    // ---- "update system and install" ----

    /// The stale card's main button.
    pub fn update_and_install(&mut self) -> Option<Ticket> {
        if self.phase != Phase::Stale {
            return None;
        }
        let request = self.request.clone();
        self.begin(request, false, true)
    }

    /// Whether this install updates the system too.
    pub fn is_updating(&self) -> bool {
        self.updating
    }

    /// Updates are waiting, on the stale card.
    pub fn waiting(&self) -> Option<Waiting> {
        self.waiting
    }

    pub fn system(&self) -> Option<&SystemPart> {
        self.system.as_ref()
    }

    /// The review follows a list that changed under the user.
    pub fn lists_changed(&self) -> bool {
        self.lists_changed
    }

    /// A ticket the end of a job asks for.
    pub fn take_follow_up(&mut self) -> Option<Ticket> {
        self.follow_up.take()
    }

    /// After a successful update and install: a restart is a good idea.
    pub fn restart_wanted(&self) -> bool {
        self.restart_wanted
    }

    pub fn system_rows_open(&self) -> bool {
        self.system_rows_open
    }

    /// The "System update" row was pressed.
    pub fn toggle_system_rows(&mut self) {
        if self.updating && matches!(self.phase, Phase::Ready(_)) {
            self.system_rows_open = !self.system_rows_open;
        }
    }

    /// The helper found other lists than the ones reviewed: look at them again, keeping every answer. The resolver
    /// drops an answer that no longer fits and asks that question again.
    fn look_again(&mut self) {
        for step in &mut self.steps {
            step.round = 0;
        }
        self.system = None;
        self.system_rows_open = false;
        self.lists_changed = true;
        self.refresh_why = Some(RefreshWhy::AfterRefusal);
        self.follow_up = self.start(false);
    }

    /// The review's Install button: `Some` means "start the helper now".
    pub fn start_install(&mut self) -> Option<JobTicket> {
        let Phase::Ready(plan) = &self.phase else {
            return None;
        };
        let plan = (**plan).clone();
        self.prompt_closed = false;
        self.refocus();
        match install_request(&plan) {
            Ok(request) => {
                self.last_number += 1;
                self.listed = plan.items.len();
                self.phase = Phase::Installing(Box::new(Installing {
                    plan,
                    job: Job::new(self.last_number),
                }));
                Some(JobTicket {
                    number: self.last_number,
                    request,
                })
            }
            Err(error) => {
                self.phase = Phase::InstallFailed(JobFailure::NotSent(error));
                None
            }
        }
    }

    /// Whether the helper is running (or its password window is open): closing the window then needs a word first.
    pub fn job_running(&self) -> bool {
        self.is_installing()
    }

    /// The running job also updates the system: the close dialog says "update" then.
    pub fn updating_job(&self) -> bool {
        self.updating && self.is_installing()
    }

    /// How many packages the confirmed list of the last run had.
    pub fn listed(&self) -> usize {
        self.listed
    }

    /// A `Progress` or `Notice` event of job `number`.
    pub fn job_event(&mut self, number: u64, event: Event) {
        if let Phase::Installing(running) = &mut self.phase
            && running.job.number == number
        {
            running.job.event(event);
        }
    }

    /// Job `number` is over. `true`: what is installed may have changed, so lists read before are old.
    pub fn job_ended(&mut self, number: u64, end: JobEnd) -> bool {
        let Phase::Installing(running) = &self.phase else {
            return false;
        };
        if running.job.number != number {
            return false;
        }
        // A result, or none at all, means pacman may have run. The other ends are all from before it.
        let may_have_changed = matches!(end, JobEnd::Finished(_) | JobEnd::NoResult { .. });
        if self.updating
            && matches!(
                end,
                JobEnd::Finished(Outcome::Error(ErrorKind::TransactionChanged))
            )
        {
            self.look_again();
            return may_have_changed;
        }
        self.restart_wanted = self.updating
            && matches!(end, JobEnd::Finished(Outcome::Ok))
            && needs_restart(running.plan.items.iter().map(|i| i.package.name.as_str()));
        self.way_back = matches!(
            end,
            JobEnd::Finished(Outcome::Error(ErrorKind::ProviderNotChosen))
        )
        .then(|| Box::new(running.plan.clone()));
        self.phase = match end {
            JobEnd::Finished(Outcome::Ok) => Phase::Installed {
                notices: running.job.notices.clone(),
            },
            JobEnd::PromptClosed => {
                self.prompt_closed = true;
                Phase::Ready(Box::new(running.plan.clone()))
            }
            end => Phase::InstallFailed(JobFailure::Ended {
                end,
                notices: running.job.notices.clone(),
            }),
        };
        self.refocus();
        may_have_changed
    }

    /// A helper job is running: nothing may leave or replace this install (there is no stop).
    fn is_installing(&self) -> bool {
        matches!(self.phase, Phase::Installing(_))
    }

    /// The password window of the last try was closed.
    pub fn prompt_was_closed(&self) -> bool {
        self.prompt_closed
    }

    /// The failure card offers "Back" to the review.
    pub fn can_return_to_review(&self) -> bool {
        self.way_back.is_some()
    }

    /// What the lists check of ticket `number` has to say, before its answer is given to `done`.
    pub fn lists_checked(&mut self, number: u64, note: Option<ListsNote>) {
        if self.wanted == Some(number) {
            self.lists_note = note;
        }
    }

    /// Why this install was checked against older lists, if it was.
    pub fn lists_note(&self) -> Option<ListsNote> {
        self.lists_note
    }

    pub fn mark_slow(&mut self) {
        if self.wanted.is_some() {
            self.slow = true;
        }
    }

    /// The worker answered the ticket `number`. An answer that is not the one wanted is dropped. Returns the
    /// waiting ticket to start next, if any.
    pub fn done(&mut self, number: u64, result: Result<WorkAnswer, PacmanError>) -> Option<Ticket> {
        let ticket = self.running.take()?;
        if ticket.number != number {
            self.running = Some(ticket);
            return None;
        }
        if self.wanted == Some(number) {
            self.wanted = None;
            self.slow = false;
            if self.apply(ticket.input.round, result) {
                // Nothing is left to update: an ordinary install, from the start.
                return self.begin(self.request.clone(), true, false);
            }
        }
        let next = self.pending.take();
        self.running.clone_from(&next);
        next
    }

    /// `true`: "update system and install" found nothing to update.
    fn apply(&mut self, round: u32, result: Result<WorkAnswer, PacmanError>) -> bool {
        match result {
            Ok(WorkAnswer::MustUpdate(waiting)) => {
                self.waiting = Some(waiting);
                self.phase = Phase::Stale;
            }
            Ok(WorkAnswer::System(check)) => {
                let SystemCheck {
                    lookup,
                    note,
                    dbpath,
                    resolution,
                } = *check;
                self.lists_note = note;
                if let Some(block) = system_block(&lookup) {
                    self.phase = Phase::SystemBlocked(block);
                } else if let UpdateLookup::Plan(plan) = lookup {
                    if plan.items.is_empty() {
                        return true;
                    }
                    self.system = Some(SystemPart {
                        plan: *plan,
                        dbpath,
                    });
                    match resolution {
                        Some(resolution) => return self.resolved(round, resolution),
                        None => {
                            self.phase = failed(&PacmanError::Inconsistent {
                                reason: "the install was not resolved",
                            });
                        }
                    }
                }
            }
            Ok(WorkAnswer::Resolved(resolution)) => return self.resolved(round, resolution),
            Err(error) => self.phase = failed(&error),
        }
        self.refocus();
        false
    }

    /// Shows what the resolver says; `true` as for `apply`.
    fn resolved(&mut self, round: u32, resolution: Resolution) -> bool {
        self.phase = match resolution {
            Resolution::Ready(plan) => match plan.decision {
                InstallDecision::MustUpdateSystem { count, .. } if !self.updating => {
                    self.waiting = Some(Waiting {
                        count,
                        lists_from: None,
                    });
                    Phase::Stale
                }
                InstallDecision::Allowed { .. } if self.updating => return true,
                _ => {
                    self.system_rows_open = false;
                    Phase::Ready(Box::new(plan))
                }
            },
            Resolution::NeedsAnswers(questions) => {
                if questions.is_empty() {
                    failed(&PacmanError::Inconsistent {
                        reason: "pacman asked for answers but named no question",
                    })
                } else {
                    // A question asked a second time replaces the answer that no longer fits.
                    self.steps
                        .retain(|step| !questions.iter().any(|q| same_subject(q, &step.question)));
                    self.current = self.steps.len();
                    for question in questions {
                        self.steps.push(Step {
                            round,
                            draft: Draft::first_for(&question),
                            question,
                        });
                    }
                    Phase::Asking
                }
            }
            Resolution::Blocked(blockers) => Phase::Blocked(blockers),
        };
        self.refocus();
        false
    }
}

/// Do two questions ask about the same thing?
fn same_subject(a: &Question, b: &Question) -> bool {
    match (a, b) {
        (
            Question::ChooseProvider { dependency: a, .. },
            Question::ChooseProvider { dependency: b, .. },
        ) => a == b,
        (
            Question::ChooseGroupMembers { group: a, .. },
            Question::ChooseGroupMembers { group: b, .. },
        ) => a == b,
        (
            Question::AlreadyInstalled { target: a, .. },
            Question::AlreadyInstalled { target: b, .. },
        ) => a == b,
        _ => false,
    }
}

/// A failed resolve. Bugs (a limit, a path, an answer that does not add up) are not worth a second try.
fn failed(error: &PacmanError) -> Phase {
    let can_retry = !matches!(
        error,
        PacmanError::TooManyRounds { .. }
            | PacmanError::BadDbPath { .. }
            | PacmanError::Inconsistent { .. }
            | PacmanError::BadPackageRef { .. }
    );
    Phase::Failed {
        text: error_text(error, Doing::Resolving),
        can_retry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper_request::RequestError;
    use tidewise_helper_protocol::{Phase as Stage, Progress};
    use tidewise_pacman::{
        Candidate, GroupAnswer, GroupChoice, InstallDecision, PlanItem, ProviderAnswer, Role,
    };

    fn pkg(name: &str) -> PackageRef {
        PackageRef {
            repo: "extra".to_string(),
            name: name.to_string(),
        }
    }

    fn provider(dependency: &str, names: &[&str], default: usize) -> Question {
        Question::ChooseProvider {
            dependency: dependency.to_string(),
            candidates: names
                .iter()
                .map(|name| Candidate {
                    package: pkg(name),
                    version: "1-1".to_string(),
                    description: format!("{name} description"),
                })
                .collect(),
            default,
        }
    }

    fn group(name: &str, members: &[(&str, Option<&str>)]) -> Question {
        Question::ChooseGroupMembers {
            group: name.to_string(),
            members: members
                .iter()
                .map(|(member, installed)| GroupChoice {
                    package: pkg(member),
                    version: "2-1".to_string(),
                    installed_version: installed.map(str::to_string),
                })
                .collect(),
        }
    }

    fn installed(name: &str, differs: bool) -> Question {
        Question::AlreadyInstalled {
            target: pkg(name),
            installed_version: "1.0-1".to_string(),
            repo_version: if differs { "0.9-1" } else { "1.0-1" }.to_string(),
            differs_without_update: differs,
        }
    }

    fn plan(decision: InstallDecision) -> InstallPlan {
        InstallPlan {
            decision,
            targets: vec![pkg("dbeaver")],
            providers: vec![ProviderAnswer {
                dependency: "java-runtime".to_string(),
                chosen: pkg("jre-openjdk"),
            }],
            items: vec![PlanItem {
                package: pkg("dbeaver"),
                version: "25-1".to_string(),
                role: Role::Target,
                installed_version: None,
                download_size: 100,
                installed_size: 200,
                installed_size_text: String::new(),
                current_size: None,
            }],
            download_total: 100,
            installed_total: 200,
        }
    }

    fn ready() -> Result<WorkAnswer, PacmanError> {
        Ok(WorkAnswer::Resolved(Resolution::Ready(plan(
            InstallDecision::Allowed { ignored: 0 },
        ))))
    }

    fn asks(questions: Vec<Question>) -> Result<WorkAnswer, PacmanError> {
        Ok(WorkAnswer::Resolved(Resolution::NeedsAnswers(questions)))
    }

    fn blocked(blockers: Vec<Blocker>) -> Result<WorkAnswer, PacmanError> {
        Ok(WorkAnswer::Resolved(Resolution::Blocked(blockers)))
    }

    /// A model with `dbeaver` opened and the first resolve answered with these questions.
    fn asking(questions: Vec<Question>) -> InstallModel {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        assert_eq!(model.done(ticket.number, asks(questions)), None);
        assert_eq!(*model.phase(), Phase::Asking);
        model
    }

    /// dbeaver reviewed and ready.
    fn reviewing() -> InstallModel {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        model.done(ticket.number, ready_with_provider());
        model
    }

    /// `plan()` with the provider in the list, as a real plan has it.
    fn ready_with_provider() -> Result<WorkAnswer, PacmanError> {
        let mut plan = plan(InstallDecision::Allowed { ignored: 0 });
        let mut provider = plan.items[0].clone();
        provider.package = pkg("jre-openjdk");
        plan.items.insert(0, provider);
        Ok(WorkAnswer::Resolved(Resolution::Ready(plan)))
    }

    fn installing() -> (InstallModel, u64) {
        let mut model = reviewing();
        let ticket = model.start_install().expect("a job");
        (model, ticket.number)
    }

    fn job(model: &InstallModel) -> &Job {
        match model.phase() {
            Phase::Installing(running) => &running.job,
            other => panic!("not installing: {other:?}"),
        }
    }

    fn step(stage: Stage, current: u32, total: u32) -> Event {
        Event::Progress(Progress {
            phase: stage,
            current,
            total,
        })
    }

    #[test]
    fn install_makes_the_request_from_the_reviewed_plan() {
        let mut model = reviewing();
        let ticket = model.start_install().expect("a job");
        assert_eq!(
            ticket.request.summary,
            "install extra/dbeaver with extra/jre-openjdk (2 packages)"
        );
        assert_eq!(job(&model).progress, None);
        assert_eq!(job(&model).percent, 0);
        // One at a time.
        assert_eq!(model.start_install(), None);
    }

    #[test]
    fn install_does_nothing_before_the_review() {
        let mut model = InstallModel::new();
        assert_eq!(model.start_install(), None);
        model.open(pkg("dbeaver"));
        assert_eq!(model.start_install(), None);
        assert_eq!(*model.phase(), Phase::Working);
    }

    #[test]
    fn a_plan_the_helper_would_refuse_is_never_sent() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        // The provider is named but not in the list: `validate` refuses it.
        model.done(ticket.number, ready());
        assert_eq!(model.start_install(), None);
        assert!(matches!(
            model.phase(),
            Phase::InstallFailed(JobFailure::NotSent(RequestError::Invalid(_)))
        ));
    }

    #[test]
    fn progress_moves_through_the_bands_and_never_backwards() {
        let (mut model, number) = installing();
        let percent = |model: &mut InstallModel, event| {
            model.job_event(number, event);
            job(model).percent
        };
        assert_eq!(percent(&mut model, step(Stage::Checking, 0, 2)), 0);
        assert_eq!(percent(&mut model, step(Stage::Downloading, 0, 2)), 5);
        assert_eq!(percent(&mut model, step(Stage::Downloading, 1, 2)), 27);
        assert_eq!(percent(&mut model, step(Stage::Downloading, 2, 2)), 50);
        assert_eq!(percent(&mut model, step(Stage::Installing, 1, 2)), 71);
        assert_eq!(percent(&mut model, step(Stage::Downloading, 0, 2)), 71);
        assert_eq!(percent(&mut model, step(Stage::Installing, 9, 2)), 92);
        assert_eq!(percent(&mut model, step(Stage::Hooks, 0, 0)), 92);
        assert_eq!(
            job(&model).progress,
            Some(Progress {
                phase: Stage::Hooks,
                current: 0,
                total: 0
            })
        );
    }

    #[test]
    fn events_of_another_job_are_ignored() {
        let (mut model, number) = installing();
        model.job_event(number + 1, step(Stage::Installing, 1, 1));
        model.job_ended(number + 1, JobEnd::Finished(Outcome::Ok));
        assert_eq!(job(&model).progress, None);
    }

    #[test]
    fn finished_ok_is_installed_and_keeps_the_notices() {
        let (mut model, number) = installing();
        model.job_event(number, Event::Notice(Notice::AuditEntryNotWritten));
        model.job_ended(number, JobEnd::Finished(Outcome::Ok));
        assert_eq!(
            *model.phase(),
            Phase::Installed {
                notices: vec![Notice::AuditEntryNotWritten]
            }
        );
        // Late events change nothing.
        model.job_event(number, step(Stage::Hooks, 0, 0));
        assert!(matches!(model.phase(), Phase::Installed { .. }));
        assert_eq!(model.back(), Back::Closed);
        assert!(!model.is_open());
    }

    #[test]
    fn a_closed_password_window_goes_back_to_the_same_review_and_says_so_once() {
        let mut model = reviewing();
        let before = model.phase().clone();
        let ticket = model.start_install().expect("a job");
        model.job_ended(ticket.number, JobEnd::PromptClosed);
        assert_eq!(*model.phase(), before);
        assert!(model.prompt_was_closed());
        // The next try forgets it.
        assert!(model.start_install().is_some());
        assert!(!model.prompt_was_closed());
        // And so does leaving the card.
        let (mut model, number) = installing();
        model.job_ended(number, JobEnd::PromptClosed);
        model.back();
        assert!(!model.prompt_was_closed());
    }

    #[test]
    fn every_other_end_is_a_failure_card() {
        for end in [
            JobEnd::Finished(Outcome::Error(ErrorKind::TransactionChanged)),
            JobEnd::NotAuthorised {
                stderr: String::new(),
            },
            JobEnd::HelperMissing,
            JobEnd::NoResult {
                exit: None,
                unreadable: 0,
                stderr: String::new(),
            },
        ] {
            let (mut model, number) = installing();
            model.job_ended(number, end.clone());
            assert_eq!(
                *model.phase(),
                Phase::InstallFailed(JobFailure::Ended {
                    end,
                    notices: Vec::new()
                })
            );
            assert_eq!(model.back(), Back::Closed);
        }
    }

    #[test]
    fn a_failed_install_keeps_the_notices_that_came_first() {
        let (mut model, number) = installing();
        model.job_event(number, Event::Notice(Notice::PackagesRemoved(3)));
        let end = JobEnd::Finished(Outcome::Error(ErrorKind::DatabaseLocked));
        model.job_ended(number, end.clone());
        assert_eq!(
            *model.phase(),
            Phase::InstallFailed(JobFailure::Ended {
                end,
                notices: vec![Notice::PackagesRemoved(3)]
            })
        );
    }

    #[test]
    fn only_an_end_that_may_have_changed_the_system_asks_for_a_new_list() {
        let changed = |end: JobEnd| {
            let (mut model, number) = installing();
            model.job_ended(number, end)
        };
        assert!(changed(JobEnd::Finished(Outcome::Ok)));
        assert!(changed(JobEnd::Finished(Outcome::Error(
            ErrorKind::TransactionChanged
        ))));
        assert!(changed(JobEnd::NoResult {
            exit: None,
            unreadable: 0,
            stderr: String::new()
        }));
        for end in [
            JobEnd::PromptClosed,
            JobEnd::HelperMissing,
            JobEnd::NotAuthorised {
                stderr: String::new(),
            },
            JobEnd::HelperRefusedToStart {
                stderr: String::new(),
            },
            JobEnd::CouldNotStart {
                reason: String::new(),
            },
        ] {
            assert!(!changed(end.clone()), "{end:?}");
        }
        let (mut model, number) = installing();
        assert!(!model.job_ended(number + 1, JobEnd::Finished(Outcome::Ok)));
    }

    fn refused(kind: ErrorKind) -> (InstallModel, Phase) {
        let mut model = reviewing();
        let review = model.phase().clone();
        let job = model.start_install().expect("a job");
        model.job_ended(job.number, JobEnd::Finished(Outcome::Error(kind)));
        (model, review)
    }

    #[test]
    fn a_provider_nobody_chose_can_go_back_to_the_review() {
        // Its fixed text says "Go back and choose again".
        let (mut model, review) = refused(ErrorKind::ProviderNotChosen);
        assert!(model.can_return_to_review());
        assert_eq!(model.back(), Back::Stayed);
        assert_eq!(*model.phase(), review);
        assert!(!model.can_return_to_review());
        // From the review, back goes on as always.
        assert_eq!(model.back(), Back::Closed);
    }

    #[test]
    fn other_failures_have_no_way_back_and_close() {
        let (mut model, _) = refused(ErrorKind::TransactionChanged);
        assert!(!model.can_return_to_review());
        assert_eq!(model.back(), Back::Closed);
    }

    #[test]
    fn a_way_back_is_forgotten_with_its_install() {
        let (mut model, _) = refused(ErrorKind::ProviderNotChosen);
        model.try_again().expect("a resolve");
        assert!(!model.can_return_to_review());
        let (mut model, _) = refused(ErrorKind::ProviderNotChosen);
        model.cancel();
        assert!(!model.can_return_to_review());
    }

    #[test]
    fn nothing_leaves_or_replaces_a_running_install() {
        let (mut model, number) = installing();
        assert_eq!(model.back(), Back::Stayed);
        model.cancel();
        assert_eq!(model.open(pkg("sl")), None);
        assert_eq!(model.open_group("plasma"), None);
        assert_eq!(model.try_again(), None);
        assert_eq!(job(&model).number, number);
        assert_eq!(model.request().targets, [pkg("dbeaver")]);
    }

    #[test]
    fn check_again_after_a_failure_starts_the_same_request_afresh() {
        let mut model = asking(vec![provider("java-runtime", &["a", "b"], 0)]);
        let ticket = model.go_on().unwrap();
        model.done(ticket.number, ready_with_provider());
        let job = model.start_install().expect("a job");
        model.job_ended(job.number, JobEnd::HelperMissing);
        let again = model.try_again().expect("a resolve");
        assert!(again.check_lists);
        assert_eq!(again.input.targets, [pkg("dbeaver")]);
        assert!(again.input.provider_answers.is_empty());
        assert_eq!(again.input.round, 0);
        assert!(model.steps().is_empty());
        assert_eq!(*model.phase(), Phase::Working);
    }

    #[test]
    fn a_new_model_is_closed() {
        let model = InstallModel::new();
        assert!(!model.is_open());
        assert_eq!(*model.phase(), Phase::Closed);
    }

    #[test]
    fn opening_asks_for_a_first_ticket_that_checks_the_lists() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        assert_eq!(ticket.number, 1);
        assert!(ticket.check_lists);
        assert_eq!(ticket.input.targets, [pkg("dbeaver")]);
        assert_eq!(ticket.input.round, 0);
        assert!(ticket.input.groups.is_empty());
        assert_eq!(*model.phase(), Phase::Working);
        assert_eq!(model.request().targets, [pkg("dbeaver")]);
    }

    #[test]
    fn opening_a_group_asks_about_the_group() {
        let mut model = InstallModel::new();
        let ticket = model.open_group("plasma").unwrap();
        assert_eq!(ticket.input.groups, ["plasma"]);
        assert!(ticket.input.targets.is_empty());
    }

    #[test]
    fn waiting_updates_means_the_system_must_be_updated_first() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        model.done(ticket.number, Ok(WorkAnswer::MustUpdate(waiting(3))));
        assert_eq!(*model.phase(), Phase::Stale);
    }

    #[test]
    fn a_plan_that_needs_an_update_is_the_same_card() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        let stale = InstallDecision::MustUpdateSystem {
            count: 3,
            ignored: 0,
        };
        let answer = Ok(WorkAnswer::Resolved(Resolution::Ready(plan(stale))));
        model.done(ticket.number, answer);
        assert_eq!(*model.phase(), Phase::Stale);
    }

    #[test]
    fn a_plan_that_is_allowed_is_ready() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        model.done(ticket.number, ready());
        assert!(matches!(model.phase(), Phase::Ready(p) if p.items.len() == 1));
    }

    #[test]
    fn blockers_are_kept_for_the_screen() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        let blockers = vec![Blocker::NotFound(vec!["dbeaver".to_string()])];
        model.done(ticket.number, blocked(blockers.clone()));
        assert_eq!(*model.phase(), Phase::Blocked(blockers));
    }

    #[test]
    fn a_pacman_failure_can_be_tried_again_but_a_bug_cannot() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        let failed = PacmanError::Failed {
            code: Some(1),
            stderr: "boom".to_string(),
        };
        model.done(ticket.number, Err(failed));
        assert!(
            matches!(model.phase(), Phase::Failed { can_retry: true, text }
            if text.title == "pacman couldn't work out what to install")
        );
        let ticket = model.try_again().unwrap();
        assert_eq!(ticket.number, 2);
        assert!(!ticket.check_lists);
        model.done(ticket.number, Err(PacmanError::TooManyRounds { limit: 5 }));
        assert!(matches!(
            model.phase(),
            Phase::Failed {
                can_retry: false,
                ..
            }
        ));
        assert_eq!(model.try_again(), None);
    }

    #[test]
    fn questions_become_steps_that_start_at_pacmans_default() {
        let model = asking(vec![
            provider("java-runtime", &["jre-openjdk", "jre21-openjdk"], 1),
            group("plasma", &[("kwin", None), ("kate", Some("1-1"))]),
            installed("zsh", false),
            installed("vlc", true),
        ]);
        let drafts: Vec<&Draft> = model.steps().iter().map(|s| &s.draft).collect();
        assert_eq!(
            drafts,
            [
                &Draft::Provider(1),
                &Draft::Members(vec![true, false]),
                &Draft::Installed { reinstall: false },
                &Draft::Differs,
            ]
        );
        assert_eq!(model.current(), 0);
        assert!(model.steps().iter().all(|s| s.round == 0));
    }

    #[test]
    fn the_input_holds_every_answer_given() {
        let mut model = asking(vec![
            provider("java-runtime", &["jre-openjdk", "jre21-openjdk"], 0),
            group(
                "plasma",
                &[("kwin", None), ("kate", Some("1-1")), ("konsole", None)],
            ),
        ]);
        model.choose(1);
        model.go_on();
        model.toggle(2);
        let input = model.input();
        assert_eq!(input.targets, [pkg("dbeaver")]);
        assert_eq!(
            input.provider_answers,
            [ProviderAnswer {
                dependency: "java-runtime".to_string(),
                chosen: pkg("jre21-openjdk"),
            }]
        );
        assert_eq!(
            input.group_answers,
            [GroupAnswer {
                group: "plasma".to_string(),
                members: vec![pkg("kwin")],
            }]
        );
        assert_eq!(input.round, 1);
    }

    #[test]
    fn leaving_an_installed_package_removes_it_and_reinstalling_names_it() {
        let mut model = asking(vec![installed("dbeaver", false)]);
        // "Leave it as it is" is the starting answer
        assert!(model.input().targets.is_empty());
        assert!(model.input().reinstall.is_empty());
        // choose(0) = leave it, choose(1) = reinstall
        model.choose(1);
        assert_eq!(model.input().reinstall, [pkg("dbeaver")]);
        assert_eq!(model.input().targets, [pkg("dbeaver")]);
        model.choose(0);
        assert!(model.input().targets.is_empty());
        assert!(model.input().reinstall.is_empty());
    }

    #[test]
    fn an_installed_package_of_another_version_is_left_out() {
        let model = asking(vec![installed("dbeaver", true)]);
        assert!(model.input().targets.is_empty());
    }

    #[test]
    fn next_goes_through_the_steps_and_sends_one_resolve_after_the_last() {
        let mut model = asking(vec![
            provider("a", &["a1", "a2"], 0),
            provider("b", &["b1", "b2"], 0),
        ]);
        assert_eq!(model.go_on(), None);
        assert_eq!(model.current(), 1);
        let ticket = model.go_on().unwrap();
        assert_eq!(ticket.number, 2);
        assert!(!ticket.check_lists);
        assert_eq!(ticket.input.round, 1);
        assert_eq!(ticket.input.provider_answers.len(), 2);
        assert_eq!(*model.phase(), Phase::Working);
    }

    #[test]
    fn a_new_round_adds_steps_after_the_answered_ones() {
        let mut model = asking(vec![provider("a", &["a1", "a2"], 0)]);
        let ticket = model.go_on().unwrap();
        model.done(
            ticket.number,
            asks(vec![
                provider("b", &["b1", "b2"], 1),
                provider("c", &["c1", "c2"], 0),
            ]),
        );
        assert_eq!(*model.phase(), Phase::Asking);
        assert_eq!(model.steps().len(), 3);
        assert_eq!(model.current(), 1);
        let rounds: Vec<u32> = model.steps().iter().map(|s| s.round).collect();
        assert_eq!(rounds, [0, 1, 1]);
    }

    #[test]
    fn going_back_keeps_the_answers() {
        let mut model = asking(vec![
            provider("a", &["a1", "a2"], 0),
            provider("b", &["b1", "b2"], 0),
        ]);
        model.choose(1);
        model.go_on();
        assert_eq!(model.back(), Back::Stayed);
        assert_eq!(model.current(), 0);
        assert_eq!(model.steps()[0].draft, Draft::Provider(1));
    }

    #[test]
    fn back_on_the_first_question_leaves_the_install() {
        let mut model = asking(vec![provider("a", &["a1", "a2"], 0)]);
        assert_eq!(model.back(), Back::Closed);
        assert!(!model.is_open());
    }

    #[test]
    fn changing_an_earlier_answer_drops_the_later_rounds() {
        let mut model = asking(vec![provider("a", &["a1", "a2"], 0)]);
        let ticket = model.go_on().unwrap();
        model.done(ticket.number, asks(vec![provider("b", &["b1", "b2"], 0)]));
        model.choose(1);
        assert_eq!(model.steps().len(), 2);
        // back to round 0, change the answer: the round-1 step and its answer are gone
        model.back();
        model.choose(1);
        assert_eq!(model.steps().len(), 1);
        assert_eq!(model.input().provider_answers.len(), 1);
        assert_eq!(model.input().round, 1);
    }

    #[test]
    fn repeating_an_answer_keeps_the_later_rounds() {
        let mut model = asking(vec![provider("a", &["a1", "a2"], 0)]);
        let ticket = model.go_on().unwrap();
        model.done(ticket.number, asks(vec![provider("b", &["b1", "b2"], 0)]));
        model.back();
        model.choose(0);
        assert_eq!(model.steps().len(), 2);
    }

    #[test]
    fn a_group_cannot_continue_with_no_members_ticked() {
        let mut model = asking(vec![group("plasma", &[("kwin", None), ("kate", None)])]);
        model.select_none();
        assert_eq!(model.steps()[0].draft, Draft::Members(vec![false, false]));
        assert_eq!(model.go_on(), None);
        assert_eq!(*model.phase(), Phase::Asking);
        model.select_all();
        assert_eq!(model.steps()[0].draft, Draft::Members(vec![true, true]));
        assert!(model.go_on().is_some());
    }

    #[test]
    fn installed_members_are_never_ticked() {
        let mut model = asking(vec![group(
            "plasma",
            &[("kwin", Some("1-1")), ("kate", None)],
        )]);
        model.toggle(0);
        model.select_all();
        assert_eq!(model.steps()[0].draft, Draft::Members(vec![false, true]));
        model.toggle(1);
        assert_eq!(model.steps()[0].draft, Draft::Members(vec![false, false]));
    }

    #[test]
    fn the_only_target_of_another_version_has_no_way_on() {
        let mut model = asking(vec![installed("dbeaver", true)]);
        assert_eq!(model.go_on(), None);
        assert_eq!(*model.phase(), Phase::Asking);
    }

    #[test]
    fn leaving_out_a_package_that_is_not_the_only_target_still_goes_on() {
        // dbeaver is the request; the question is about another package (say a dependency's installed copy)
        let model = asking(vec![
            installed("vlc", true),
            provider("a", &["a1", "a2"], 0),
        ]);
        assert!(model.can_go_on());
    }

    #[test]
    fn an_answer_for_an_old_ticket_is_ignored() {
        let mut model = InstallModel::new();
        let first = model.open(pkg("dbeaver")).unwrap();
        model.cancel();
        assert!(!model.is_open());
        // asked again while the first still runs: it waits
        assert_eq!(model.open(pkg("vlc")), None);
        // the first one finishes; its answer is dropped, and the waiting one starts
        let second = model.done(first.number, ready()).unwrap();
        assert_eq!(second.input.targets, [pkg("vlc")]);
        assert_eq!(*model.phase(), Phase::Working);
    }

    #[test]
    fn a_second_ticket_waits_while_one_runs() {
        let mut model = InstallModel::new();
        let first = model.open(pkg("a")).unwrap();
        assert_eq!(model.open(pkg("b")), None);
        assert_eq!(first.number, 1);
        // the running one is still the only one that ran
        let waiting = model.done(first.number, ready());
        assert_eq!(waiting.map(|t| t.number), Some(2));
    }

    #[test]
    fn a_third_ticket_replaces_the_waiting_second() {
        let mut model = InstallModel::new();
        let first = model.open(pkg("a")).unwrap();
        assert_eq!(model.open(pkg("b")), None);
        assert_eq!(model.open(pkg("c")), None);
        let next = model.done(first.number, ready()).unwrap();
        assert_eq!(next.number, 3);
        assert_eq!(next.input.targets, [pkg("c")]);
        // never two at once: nothing else waits
        assert_eq!(model.done(next.number, ready()), None);
    }

    #[test]
    fn an_answer_that_is_not_running_changes_nothing() {
        let mut model = InstallModel::new();
        let first = model.open(pkg("a")).unwrap();
        assert_eq!(model.done(99, ready()), None);
        assert_eq!(*model.phase(), Phase::Working);
        model.done(first.number, ready());
        assert!(matches!(model.phase(), Phase::Ready(_)));
        // a second answer for the same ticket is ignored
        model.done(first.number, Ok(WorkAnswer::MustUpdate(waiting(3))));
        assert!(matches!(model.phase(), Phase::Ready(_)));
    }

    #[test]
    fn going_back_while_working_drops_the_answer() {
        let mut model = asking(vec![provider("a", &["a1", "a2"], 0)]);
        let ticket = model.go_on().unwrap();
        assert_eq!(model.back(), Back::Stayed);
        assert_eq!(*model.phase(), Phase::Asking);
        assert_eq!(model.current(), 0);
        model.done(ticket.number, ready());
        assert_eq!(*model.phase(), Phase::Asking);
    }

    #[test]
    fn back_from_a_result_returns_to_the_last_question_or_leaves() {
        let mut model = asking(vec![provider("a", &["a1", "a2"], 0)]);
        let ticket = model.go_on().unwrap();
        model.done(ticket.number, ready());
        assert_eq!(model.back(), Back::Stayed);
        assert_eq!(*model.phase(), Phase::Asking);
        assert_eq!(model.current(), 0);

        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        model.done(ticket.number, ready());
        assert_eq!(model.back(), Back::Closed);
        assert!(!model.is_open());
    }

    #[test]
    fn a_shared_library_candidate_starts_a_normal_install_of_it() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("carla")).unwrap();
        let blockers = vec![Blocker::SonameChoice {
            dependency: "libjack.so".to_string(),
            candidates: vec![pkg("jack2"), pkg("pipewire-jack")],
        }];
        assert_eq!(model.install_choice(0), None);
        model.done(ticket.number, blocked(blockers));
        assert_eq!(model.install_choice(2), None);
        let ticket = model.install_choice(1).unwrap();
        assert_eq!(ticket.input.targets, [pkg("pipewire-jack")]);
        assert_eq!(ticket.input.round, 0);
        // A fresh install: the updates check runs again.
        assert!(ticket.check_lists);
        assert_eq!(model.request().targets, [pkg("pipewire-jack")]);
    }

    #[test]
    fn install_instead_starts_again_with_the_replacement() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("cron")).unwrap();
        let blockers = vec![Blocker::ReplacedBy {
            requested: pkg("cron"),
            by: vec![pkg("cronie")],
        }];
        model.done(ticket.number, blocked(blockers));
        let ticket = model.install_instead().unwrap();
        assert_eq!(ticket.input.targets, [pkg("cronie")]);
        assert_eq!(ticket.input.round, 0);
        assert!(!ticket.check_lists);
        assert_eq!(model.request().targets, [pkg("cronie")]);
        assert!(model.steps().is_empty());
    }

    #[test]
    fn install_instead_needs_exactly_one_replacement() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("cron")).unwrap();
        let blockers = vec![Blocker::ReplacedBy {
            requested: pkg("cron"),
            by: vec![pkg("cronie"), pkg("fcron")],
        }];
        model.done(ticket.number, blocked(blockers));
        assert_eq!(model.install_instead(), None);
    }

    #[test]
    fn slow_counts_only_while_a_resolve_is_wanted() {
        let mut model = InstallModel::new();
        model.mark_slow();
        assert!(!model.is_slow());
        let ticket = model.open(pkg("dbeaver")).unwrap();
        model.mark_slow();
        assert!(model.is_slow());
        model.done(ticket.number, ready());
        assert!(!model.is_slow());
    }

    #[test]
    fn every_new_card_moves_the_focus() {
        let mut model = InstallModel::new();
        let start = model.focus_serial();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        let working = model.focus_serial();
        assert_ne!(start, working);
        model.done(
            ticket.number,
            asks(vec![
                provider("a", &["a1", "a2"], 0),
                provider("b", &["b1", "b2"], 0),
            ]),
        );
        let first = model.focus_serial();
        assert_ne!(working, first);
        model.go_on();
        assert_ne!(first, model.focus_serial());
    }

    fn note(why: tidewise_pacman::NotFresh) -> ListsNote {
        ListsNote {
            why,
            age_days: Some(2),
        }
    }

    #[test]
    fn the_lists_note_is_kept_while_this_install_is_open_and_gone_with_the_next() {
        let mut model = InstallModel::new();
        let first = model.open(pkg("dbeaver")).expect("starts at once");
        assert_eq!(model.lists_note(), None);
        let busy = note(tidewise_pacman::NotFresh::Busy);
        model.lists_checked(first.number, Some(busy));
        model.done(first.number, Ok(WorkAnswer::MustUpdate(waiting(3))));
        assert_eq!(model.lists_note(), Some(busy));
        model.open(pkg("cronie"));
        assert_eq!(model.lists_note(), None);
    }

    #[test]
    fn a_note_from_a_ticket_nobody_wants_any_more_is_dropped() {
        let mut model = InstallModel::new();
        let first = model.open(pkg("dbeaver")).expect("starts at once");
        model.open(pkg("cronie"));
        model.lists_checked(first.number, Some(note(tidewise_pacman::NotFresh::Failed)));
        assert_eq!(model.lists_note(), None);
    }

    // ---- "update system and install" (WBS 6.6) ------------------------------------------------------------------

    const FOLDER: &str = "/run/user/1000/tidewise/lists/";

    fn waiting(count: usize) -> Waiting {
        Waiting {
            count,
            lists_from: Some(SystemTime::UNIX_EPOCH),
        }
    }

    fn must_update() -> InstallDecision {
        InstallDecision::MustUpdateSystem {
            count: 3,
            ignored: 0,
        }
    }

    fn item(name: &str, role: Role) -> PlanItem {
        PlanItem {
            package: pkg(name),
            version: "2-1".to_string(),
            role,
            installed_version: (role == Role::Update).then(|| "1-1".to_string()),
            download_size: 100,
            installed_size: 200,
            installed_size_text: String::new(),
            current_size: None,
        }
    }

    /// The system update brings `linux`; the install adds `dbeaver` and its Java.
    fn combined() -> InstallPlan {
        let mut plan = plan(must_update());
        plan.items = vec![
            item("linux", Role::Update),
            item("jre-openjdk", Role::Provider),
            item("dbeaver", Role::Target),
        ];
        plan
    }

    fn system_plan(names: &[&str]) -> UpdatePlan {
        UpdatePlan {
            items: names.iter().map(|name| item(name, Role::Update)).collect(),
            ..UpdatePlan::default()
        }
    }

    fn checked_with(
        lookup: UpdateLookup,
        resolution: Option<Resolution>,
    ) -> Result<WorkAnswer, PacmanError> {
        Ok(WorkAnswer::System(Box::new(SystemCheck {
            lookup,
            note: None,
            dbpath: Some(PathBuf::from(FOLDER)),
            resolution,
        })))
    }

    fn checked(resolution: Resolution) -> Result<WorkAnswer, PacmanError> {
        checked_with(
            UpdateLookup::Plan(Box::new(system_plan(&["linux"]))),
            Some(resolution),
        )
    }

    /// The stale card, accepted: the model and the ticket of the first step.
    fn accepted() -> (InstallModel, Ticket) {
        let mut model = InstallModel::new();
        let first = model.open(pkg("dbeaver")).unwrap();
        model.done(first.number, Ok(WorkAnswer::MustUpdate(waiting(3))));
        let ticket = model.update_and_install().expect("a ticket");
        (model, ticket)
    }

    /// The combined review.
    fn combined_review() -> InstallModel {
        let (mut model, ticket) = accepted();
        model.done(ticket.number, checked(Resolution::Ready(combined())));
        model
    }

    #[test]
    fn the_stale_card_remembers_what_is_waiting() {
        let mut model = InstallModel::new();
        let first = model.open(pkg("dbeaver")).unwrap();
        model.done(first.number, Ok(WorkAnswer::MustUpdate(waiting(143))));
        assert_eq!(model.waiting(), Some(waiting(143)));
        // A resolve that finds updates waiting says how many, without a date.
        let mut model = InstallModel::new();
        let first = model.open(pkg("dbeaver")).unwrap();
        let resolved = Resolution::Ready(plan(must_update()));
        model.done(first.number, Ok(WorkAnswer::Resolved(resolved)));
        assert_eq!(*model.phase(), Phase::Stale);
        assert_eq!(
            model.waiting(),
            Some(Waiting {
                count: 3,
                lists_from: None
            })
        );
        assert!(!model.is_updating());
    }

    #[test]
    fn accepting_the_stale_card_asks_for_the_system_step() {
        let (model, ticket) = accepted();
        assert_eq!(ticket.refresh_system, Some(RefreshWhy::Opening));
        assert!(!ticket.check_lists);
        assert_eq!(ticket.input.targets, [pkg("dbeaver")]);
        assert_eq!(ticket.input.dbpath, None);
        assert_eq!(*model.phase(), Phase::Working);
        assert!(model.is_updating());
    }

    #[test]
    fn a_plan_that_updates_the_system_is_the_review_once_accepted() {
        let model = combined_review();
        assert_eq!(*model.phase(), Phase::Ready(Box::new(combined())));
        assert_eq!(
            model.system().map(|s| (&s.plan, s.dbpath.as_deref())),
            Some((&system_plan(&["linux"]), Some(std::path::Path::new(FOLDER))))
        );
    }

    #[test]
    fn questions_are_asked_against_the_same_private_copy() {
        let (mut model, ticket) = accepted();
        let questions = vec![provider("java-runtime", &["jre-openjdk", "jre17"], 0)];
        model.done(ticket.number, checked(Resolution::NeedsAnswers(questions)));
        assert_eq!(*model.phase(), Phase::Asking);
        let next = model.go_on().expect("a resolve");
        assert_eq!(
            next.refresh_system, None,
            "no second refresh between rounds"
        );
        assert!(!next.check_lists);
        assert_eq!(next.input.dbpath, Some(PathBuf::from(FOLDER)));
        assert_eq!(next.input.provider_answers.len(), 1);
        let resolved = Resolution::Ready(combined());
        model.done(next.number, Ok(WorkAnswer::Resolved(resolved)));
        assert!(matches!(model.phase(), Phase::Ready(_)));
    }

    #[test]
    fn a_system_part_that_cannot_go_ahead_blocks_and_can_be_checked_again() {
        let held = tidewise_pacman::HeldBlock {
            package: "a".to_string(),
            held: "b".to_string(),
        };
        let replacement = tidewise_pacman::Replacement {
            new: pkg("new"),
            old: "old".to_string(),
        };
        let conflict = tidewise_pacman::Conflict {
            package: "a".to_string(),
            installed: "b".to_string(),
        };
        let replaced = UpdatePlan {
            replacements: vec![replacement.clone()],
            ..system_plan(&["linux"])
        };
        let conflicting = UpdatePlan {
            conflicts: vec![conflict.clone()],
            ..system_plan(&["linux"])
        };
        for (lookup, block) in [
            (
                UpdateLookup::HeldBlock(held.clone()),
                UpdateBlock::Held(held),
            ),
            (
                UpdateLookup::Plan(Box::new(replaced)),
                UpdateBlock::Replacement(vec![replacement]),
            ),
            (
                UpdateLookup::Plan(Box::new(conflicting)),
                UpdateBlock::Conflict(vec![conflict]),
            ),
        ] {
            let (mut model, ticket) = accepted();
            model.done(ticket.number, checked_with(lookup, None));
            assert_eq!(*model.phase(), Phase::SystemBlocked(block));
            let again = model.try_again().expect("check again");
            assert_eq!(again.refresh_system, Some(RefreshWhy::Opening));
        }
    }

    #[test]
    fn a_choice_the_system_update_brings_does_not_block_update_and_install() {
        // A plain update sends this to a terminal; here the resolver names every pick (decision 0009).
        let picking = UpdatePlan {
            picks: vec![tidewise_pacman::ProviderPick {
                dependency: "java-runtime".to_string(),
                provider: pkg("jre-openjdk"),
                version: None,
            }],
            ..system_plan(&["linux"])
        };
        let (mut model, ticket) = accepted();
        let lookup = UpdateLookup::Plan(Box::new(picking));
        let resolved = Some(Resolution::Ready(combined()));
        model.done(ticket.number, checked_with(lookup, resolved));
        assert!(matches!(model.phase(), Phase::Ready(_)));
    }

    #[test]
    fn nothing_left_to_update_goes_on_as_a_plain_install() {
        let (mut model, ticket) = accepted();
        let lookup = UpdateLookup::Plan(Box::default());
        let resolved = Resolution::Ready(plan(InstallDecision::Allowed { ignored: 0 }));
        let next = model
            .done(ticket.number, checked_with(lookup, Some(resolved)))
            .expect("start again as a plain install");
        assert!(next.check_lists);
        assert_eq!(next.refresh_system, None);
        assert_eq!(next.input.dbpath, None);
        assert!(!model.is_updating());
        assert_eq!(*model.phase(), Phase::Working);
    }

    #[test]
    fn a_private_copy_that_could_not_be_used_says_so_on_the_review() {
        let note = ListsNote {
            why: tidewise_pacman::NotFresh::Failed,
            age_days: Some(4),
        };
        let (mut model, ticket) = accepted();
        let check = SystemCheck {
            lookup: UpdateLookup::Plan(Box::new(system_plan(&["linux"]))),
            note: Some(note),
            dbpath: None,
            resolution: Some(Resolution::Ready(combined())),
        };
        model.done(ticket.number, Ok(WorkAnswer::System(Box::new(check))));
        assert_eq!(model.lists_note(), Some(note));
        assert!(matches!(model.phase(), Phase::Ready(_)));
    }

    #[test]
    fn a_failed_first_step_is_tried_again_from_the_refresh() {
        let (mut model, ticket) = accepted();
        let error = PacmanError::Failed {
            code: Some(1),
            stderr: "x".to_string(),
        };
        model.done(ticket.number, Err(error));
        let again = model.try_again().expect("try again");
        assert_eq!(again.refresh_system, Some(RefreshWhy::Opening));
    }

    #[test]
    fn lists_that_changed_after_the_password_are_checked_again_with_the_answers_kept() {
        let (mut model, ticket) = accepted();
        let questions = vec![provider("java-runtime", &["jre-openjdk", "jre17"], 0)];
        model.done(ticket.number, checked(Resolution::NeedsAnswers(questions)));
        let next = model.go_on().unwrap();
        model.done(
            next.number,
            Ok(WorkAnswer::Resolved(Resolution::Ready(combined()))),
        );
        let job = model.start_install().expect("a job");
        assert!(
            job.request
                .summary
                .starts_with("update system and install ")
        );
        let end = JobEnd::Finished(Outcome::Error(ErrorKind::TransactionChanged));
        assert!(model.job_ended(job.number, end));
        let again = model.take_follow_up().expect("look again");
        assert_eq!(again.refresh_system, Some(RefreshWhy::AfterRefusal));
        assert_eq!(again.input.provider_answers.len(), 1);
        assert_eq!(model.take_follow_up(), None);
        model.done(again.number, checked(Resolution::Ready(combined())));
        assert!(matches!(model.phase(), Phase::Ready(_)));
        assert!(model.lists_changed());
    }

    #[test]
    fn a_question_asked_again_replaces_the_old_answer() {
        let (mut model, ticket) = accepted();
        let first = vec![provider("java-runtime", &["jre-openjdk", "jre17"], 0)];
        model.done(ticket.number, checked(Resolution::NeedsAnswers(first)));
        let next = model.go_on().unwrap();
        // The resolver drops an answer that no longer fits and asks the same thing again.
        let second = vec![provider("java-runtime", &["jre21", "jre25"], 0)];
        model.done(next.number, asks(second));
        assert_eq!(model.steps().len(), 1);
        let last = model.go_on().unwrap();
        assert_eq!(last.input.provider_answers.len(), 1);
        assert_eq!(last.input.provider_answers[0].chosen, pkg("jre21"));
    }

    #[test]
    fn installing_a_shared_library_choice_goes_on_as_update_and_install_of_it() {
        let (mut model, ticket) = accepted();
        let choice = Resolution::Blocked(vec![Blocker::SonameChoice {
            dependency: "libjack.so".to_string(),
            candidates: vec![pkg("jack2"), pkg("pipewire-jack")],
        }]);
        model.done(ticket.number, checked(choice));
        let next = model.install_choice(1).expect("the candidate");
        // One path: no stale card in between, straight to the system step of the candidate.
        assert!(model.is_updating());
        assert_eq!(next.input.targets, [pkg("pipewire-jack")]);
        assert_eq!(next.refresh_system, Some(RefreshWhy::Opening));
        assert!(!next.check_lists);
        model.done(next.number, checked(Resolution::Ready(combined())));
        assert!(matches!(model.phase(), Phase::Ready(_)));
    }

    #[test]
    fn leaving_forgets_that_the_system_was_to_be_updated() {
        let mut model = combined_review();
        model.cancel();
        assert!(!model.is_updating());
        assert_eq!(model.system(), None);
        let ticket = model.open(pkg("zsh")).unwrap();
        assert!(ticket.check_lists);
        assert_eq!(ticket.input.dbpath, None);
    }

    #[test]
    fn a_finished_update_and_install_asks_for_a_restart_after_a_core_part() {
        for (name, restart) in [("linux", true), ("zsh", false)] {
            let mut plan = combined();
            plan.items[0] = item(name, Role::Update);
            let (mut model, ticket) = accepted();
            model.done(ticket.number, checked(Resolution::Ready(plan)));
            let job = model.start_install().unwrap();
            model.job_ended(job.number, JobEnd::Finished(Outcome::Ok));
            assert_eq!(model.restart_wanted(), restart, "{name}");
        }
        // A plain install never says it.
        let (mut model, number) = installing();
        model.job_ended(number, JobEnd::Finished(Outcome::Ok));
        assert!(!model.restart_wanted());
    }

    #[test]
    fn the_system_row_is_closed_until_pressed() {
        let mut model = combined_review();
        assert!(!model.system_rows_open());
        model.toggle_system_rows();
        assert!(model.system_rows_open());
        model.toggle_system_rows();
        assert!(!model.system_rows_open());
    }

    #[test]
    fn a_running_update_and_install_is_an_update_job() {
        let mut model = combined_review();
        assert!(!model.updating_job());
        model.start_install().unwrap();
        assert!(model.updating_job());
        let (model, _) = installing();
        assert!(!model.updating_job(), "a plain install is not");
    }
}
