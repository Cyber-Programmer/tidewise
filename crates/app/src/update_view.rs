//! Turns the `UpdateModel` into the exact words and flags the update screen shows (WBS 6.5). No Slint types here, so
//! every string the window displays is unit-tested. Blocked-card, skipped, held and refusal words are the human's
//! (board M4UpdateSystem).

use crate::failure_words::{DOWNLOAD_FAILED, download_failure, nothing_installed, notice_line};
use crate::helper_run::JobEnd;
use crate::install_view::{
    ButtonView, FocusTarget, ReviewRow, StageState, StageView, TileView, download_tile, lists_note,
    size_tile, technical_item,
};
use crate::job::{Job, JobFailure};
use crate::size_text::format_size;
use crate::update_model::{Phase, UpdateModel};
use crate::update_words::{
    HELD_TITLE, HELD_WARNING, LISTS_CHANGED, NEWS_LINE, NOTHING_REMOVED, RESTART_LINE, UP_TO_DATE,
    UpdateBlock, block_words, held_line, refusal_body,
};
use tidewise_helper_protocol::{ErrorKind, Notice, Outcome, Phase as Stage};
use tidewise_pacman::{PlanItem, UpdatePlan};

/// Which card fills the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpdateKind {
    /// Nothing asked yet: "Check for updates".
    #[default]
    Start,
    Checking,
    Review,
    UpToDate,
    /// A held package, a conflict or a pick: nothing can be updated here.
    Blocked,
    /// The check failed, or the update did.
    Alert,
    Progress,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateView {
    pub kind: UpdateKind,
    pub title: String,
    pub body: String,
    /// What to run in a terminal, or empty.
    pub advice: String,
    /// Review: updates, download, disk.
    pub tiles: Vec<TileView>,
    /// Every package the update installs, in pacman's order.
    pub rows: Vec<ReviewRow>,
    /// Empty when nothing is held.
    pub held_title: String,
    pub held_lines: Vec<String>,
    pub held_warning: String,
    /// "Nothing will be removed.", the news line, and what is said about older or changed lists.
    pub notes: Vec<String>,
    pub primary: Option<ButtonView>,
    pub secondary: Option<ButtonView>,
    pub primary_note: String,
    pub hint: String,
    pub technical: Vec<String>,
    pub stages: Vec<StageView>,
    pub percent: u8,
    pub percent_text: String,
    pub attention: bool,
    pub focus: FocusTarget,
    pub focus_serial: u32,
}

pub fn update_view(model: &UpdateModel) -> UpdateView {
    let mut view = UpdateView {
        focus_serial: model.focus_serial(),
        ..UpdateView::default()
    };
    match model.phase() {
        Phase::Closed => {
            view.title = HEADING.to_string();
            view.body = "See what a full update would change before anything happens.".to_string();
            view.primary = button("Check for updates");
        }
        Phase::Checking => {
            view.kind = UpdateKind::Checking;
            view.title = "Refreshing package lists…".to_string();
            view.body = "Nothing on your system is changed.".to_string();
            view.secondary = button("Cancel");
        }
        Phase::Review(plan) => review(&mut view, model, plan),
        Phase::UpToDate(plan) => {
            view.kind = UpdateKind::UpToDate;
            view.title = UP_TO_DATE.to_string();
            held(&mut view, plan);
            view.notes.extend(model.lists_note().map(lists_note));
            view.primary = button("Check again");
        }
        Phase::Blocked(block) => {
            let words = block_words(block);
            view.kind = UpdateKind::Blocked;
            view.title = words.title;
            view.body = words.body;
            view.advice = words.advice;
            if let UpdateBlock::Replacement(replacements) = block {
                view.technical = replacements
                    .iter()
                    .map(|r| format!("{}/{} replaces {}", r.new.repo, r.new.name, r.old))
                    .collect();
            }
            view.primary = button("Check again");
            view.secondary = button("Close");
        }
        Phase::Failed { text } => {
            view.kind = UpdateKind::Alert;
            view.title.clone_from(&text.title);
            view.body.clone_from(&text.body);
            view.technical = vec![text.technical.clone()];
            view.primary = button("Check again");
            view.secondary = button("Close");
        }
        Phase::Updating(running) => progress(&mut view, &running.job),
        Phase::Updated { notices, restart } => done(&mut view, notices, *restart, model.listed()),
        Phase::UpdateFailed(failure) => failed(&mut view, failure, model.listed()),
    }
    view.focus = match view.kind {
        UpdateKind::Review => FocusTarget::List,
        _ if view.primary.is_some() => FocusTarget::Primary,
        _ => FocusTarget::Secondary,
    };
    view
}

const HEADING: &str = "Update system";

fn button(label: &str) -> Option<ButtonView> {
    Some(ButtonView {
        label: label.to_string(),
        enabled: true,
    })
}

/// What the update leaves alone: what `IgnorePkg` holds.
fn held(view: &mut UpdateView, plan: &UpdatePlan) {
    if !plan.held_back.is_empty() {
        view.held_title = HELD_TITLE.to_string();
        view.held_lines = plan.held_back.iter().map(held_line).collect();
        view.held_warning = HELD_WARNING.to_string();
    }
}

fn review(view: &mut UpdateView, model: &UpdateModel, plan: &UpdatePlan) {
    view.kind = UpdateKind::Review;
    view.title = "Review the update".to_string();
    view.rows = plan.items.iter().map(row).collect();
    let count = view.rows.len();
    let change = plan.size_change();
    let disk = if change < 0 {
        "Disk space freed"
    } else {
        "Extra disk space"
    };
    view.tiles = vec![
        TileView {
            label: "Packages".to_string(),
            number: count.to_string(),
            ..TileView::default()
        },
        download_tile(plan.download_total()),
        size_tile(disk, change.unsigned_abs()),
    ];
    held(view, plan);
    if model.lists_changed() {
        view.notes.push(LISTS_CHANGED.to_string());
    }
    view.notes.push(NOTHING_REMOVED.to_string());
    view.notes.push(NEWS_LINE.to_string());
    view.notes.extend(model.lists_note().map(lists_note));
    view.technical = plan.items.iter().map(technical_item).collect();
    view.technical
        .push("pacman counts in MiB (1 MiB = 1,048,576 bytes)".to_string());
    let noun = if count == 1 { "package" } else { "packages" };
    view.primary = button(&format!("Update {count} {noun}"));
    view.secondary = button("Cancel");
    view.hint = "Your desktop asks for your password once.".to_string();
    if model.prompt_was_closed() {
        view.primary_note = "The password prompt was closed. Nothing was changed.".to_string();
    }
}

fn row(item: &PlanItem) -> ReviewRow {
    let version = match &item.installed_version {
        Some(old) => format!("{old} → {}", item.version),
        None => item.version.clone(),
    };
    let what = if item.installed_version.is_some() {
        "Update"
    } else {
        "New"
    };
    let current = item.current_size.unwrap_or(0);
    let sign = if item.installed_size < current {
        '-'
    } else {
        '+'
    };
    let change = format_size(item.installed_size.abs_diff(current));
    ReviewRow {
        name: item.package.name.clone(),
        version,
        why: format!("{what}, {sign}{change} on disk"),
        size: if item.download_size == 0 {
            "Already downloaded".to_string()
        } else {
            format_size(item.download_size)
        },
        bold: false,
    }
}

/// The helper's four phases, in order, as the progress card names them.
const STAGES: [(Stage, &str); 4] = [
    (Stage::Checking, "Checking the list again"),
    (Stage::Downloading, "Downloading packages"),
    (Stage::Installing, "Updating"),
    (Stage::Hooks, "Finishing (pacman hooks)"),
];

/// The running job. There is no button: nothing here can stop it.
fn progress(view: &mut UpdateView, job: &Job) {
    view.kind = UpdateKind::Progress;
    let now = job.progress.and_then(|progress| {
        STAGES
            .iter()
            .position(|(stage, _)| *stage == progress.phase)
    });
    view.stages = STAGES
        .iter()
        .enumerate()
        .map(|(index, (_, name))| StageView {
            label: (*name).to_string(),
            state: match now {
                Some(now) if index < now => StageState::Done,
                Some(now) if index == now => StageState::Now,
                _ => StageState::Later,
            },
        })
        .collect();
    let Some(progress) = job.progress else {
        view.title = "Waiting for your password".to_string();
        view.body = "Your desktop asks for it. Tidewise never sees your password.".to_string();
        return;
    };
    let counted = |verb: &str| {
        if progress.current == 0 {
            format!("{verb} packages")
        } else {
            format!("{verb} {} of {}", progress.current, progress.total)
        }
    };
    view.title = match progress.phase {
        Stage::Checking => "Checking the list again".to_string(),
        Stage::Downloading => counted("Downloading"),
        Stage::Installing => counted("Updating"),
        Stage::Hooks => "Finishing (pacman hooks)".to_string(),
    };
    view.percent = job.percent;
    view.percent_text = format!("{} %", job.percent);
    view.hint =
        "Updating can't be stopped halfway. Keep Tidewise open until it's done.".to_string();
}

fn done(view: &mut UpdateView, notices: &[Notice], restart: bool, listed: usize) {
    view.kind = UpdateKind::Done;
    // A package missing or gone is worth a warning; a journal entry that was not written is not.
    view.attention = notices.iter().any(|notice| {
        matches!(
            notice,
            Notice::PackagesNotInstalled(_) | Notice::PackagesRemoved(_)
        )
    });
    view.title = if view.attention {
        "The update finished"
    } else {
        UP_TO_DATE
    }
    .to_string();
    let mut lines: Vec<String> = notices
        .iter()
        .map(|notice| notice_line(notice, listed))
        .collect();
    if restart && !nothing_installed(notices, listed) {
        lines.push(RESTART_LINE.to_string());
    }
    view.body = lines.join("\n");
    view.primary = button("Done");
}

fn failed(view: &mut UpdateView, failure: &JobFailure, listed: usize) {
    view.kind = UpdateKind::Alert;
    view.title = "Tidewise couldn't update your system".to_string();
    view.primary = button("Check again");
    view.secondary = button("Close");
    let lines = |text: &str| text.lines().map(str::to_string).collect::<Vec<_>>();
    let (end, notices) = match failure {
        JobFailure::NotSent(error) => {
            view.body =
                "Tidewise made a request its helper would refuse. Nothing was changed.".to_string();
            view.technical = vec![format!("{error:?}")];
            return;
        }
        JobFailure::Ended { end, notices } => (end, notices),
    };
    match end {
        JobEnd::Finished(Outcome::Error(kind)) => {
            if let Some(details) = download_failure(kind, notices, listed) {
                view.body = DOWNLOAD_FAILED.to_string();
                view.technical = lines(details.as_str());
                return;
            }
            view.body = refusal_body(kind).to_string();
            if let ErrorKind::PacmanFailed(details) = kind {
                // pacman may have stopped halfway, or after updating: claim neither.
                view.title = "The update ended with an error".to_string();
                view.body.push_str("\nCheck Apps to see what is installed.");
                view.technical = lines(details.as_str());
            }
        }
        JobEnd::NotAuthorised { stderr } => {
            view.body = "Tidewise did not get permission to make this change. Nothing was changed."
                .to_string();
            view.technical = lines(stderr);
        }
        JobEnd::HelperMissing => {
            view.body =
                "Tidewise's helper is not installed, so Tidewise can't update your system. \
                         Reinstall Tidewise. Nothing was changed."
                    .to_string();
        }
        JobEnd::HelperRefusedToStart { stderr } => {
            view.body = "Tidewise's helper refused to start. Nothing was changed.".to_string();
            view.technical = lines(stderr);
        }
        JobEnd::CouldNotStart { reason } => {
            view.body =
                "Tidewise could not start the password window. Nothing was changed.".to_string();
            view.technical = lines(reason);
        }
        // Neither of the first two reaches this card (the model shows them elsewhere); if one did, nothing is claimed.
        JobEnd::Finished(Outcome::Ok) | JobEnd::PromptClosed => unknown_result(view),
        JobEnd::NoResult {
            exit,
            unreadable,
            stderr,
        } => {
            unknown_result(view);
            view.technical = vec![
                exit.map_or_else(
                    || "exit code: none (stopped by a signal)".to_string(),
                    |code| format!("exit code: {code}"),
                ),
                format!("unreadable lines from the helper: {unreadable}"),
            ];
            view.technical.extend(lines(stderr));
        }
    }
    for notice in notices {
        view.body.push('\n');
        view.body.push_str(&notice_line(notice, listed));
    }
}

fn unknown_result(view: &mut UpdateView) {
    view.title = "The result of the update is unknown".to_string();
    view.body = "Tidewise could not tell whether the update finished. Check Apps, or look in the \
                 system journal."
        .to_string();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_check::ListsNote;
    use tidewise_helper_protocol::{Details, Event, Progress};
    use tidewise_pacman::{
        HeldBlock, NotFresh, PackageRef, PacmanError, PendingUpdate, Role, UpdateLookup,
    };

    fn pkg(name: &str) -> PackageRef {
        PackageRef {
            repo: "extra".to_string(),
            name: name.to_string(),
        }
    }

    fn item(name: &str, old: Option<&str>) -> PlanItem {
        PlanItem {
            package: pkg(name),
            version: "2-1".to_string(),
            role: if old.is_some() {
                Role::Update
            } else {
                Role::Dependency
            },
            installed_version: old.map(str::to_string),
            download_size: 1_500_000,
            installed_size: 3_000_000,
            installed_size_text: "2.86 MiB".to_string(),
            current_size: old.map(|_| 1_000_000),
        }
    }

    fn mixed() -> UpdatePlan {
        UpdatePlan {
            items: vec![item("zsh", Some("1-1")), item("libnew", None)],
            held_back: vec![PendingUpdate {
                name: "linux".to_string(),
                installed_version: "1-1".to_string(),
                new_version: "2-1".to_string(),
                ignored: true,
            }],
            ..UpdatePlan::default()
        }
    }

    fn after_check(lookup: UpdateLookup, note: Option<ListsNote>) -> UpdateModel {
        let mut model = UpdateModel::new();
        let ticket = model.check().unwrap();
        model.checked(ticket.number, Ok((lookup, note)));
        model
    }

    fn reviewing(plan: UpdatePlan) -> UpdateModel {
        after_check(UpdateLookup::Plan(Box::new(plan)), None)
    }

    fn labels(view: &UpdateView) -> (Option<&str>, Option<&str>) {
        (
            view.primary.as_ref().map(|b| b.label.as_str()),
            view.secondary.as_ref().map(|b| b.label.as_str()),
        )
    }

    #[test]
    fn before_a_check_there_is_one_button() {
        let view = update_view(&UpdateModel::new());
        assert_eq!(view.kind, UpdateKind::Start);
        assert_eq!(view.title, "Update system");
        assert_eq!(labels(&view), (Some("Check for updates"), None));
    }

    #[test]
    fn checking_can_be_cancelled() {
        let mut model = UpdateModel::new();
        model.check();
        let view = update_view(&model);
        assert_eq!(view.kind, UpdateKind::Checking);
        assert_eq!(view.title, "Refreshing package lists…");
        assert_eq!(view.body, "Nothing on your system is changed.");
        assert_eq!(labels(&view), (None, Some("Cancel")));
        assert_eq!(view.focus, FocusTarget::Secondary);
    }

    #[test]
    fn the_review_lists_every_package_and_what_is_held() {
        let view = update_view(&reviewing(mixed()));
        assert_eq!(view.kind, UpdateKind::Review);
        assert_eq!(view.title, "Review the update");
        let rows: Vec<(&str, &str, &str)> = view
            .rows
            .iter()
            .map(|r| (r.name.as_str(), r.version.as_str(), r.why.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("zsh", "1-1 → 2-1", "Update, +2.0 MB on disk"),
                ("libnew", "2-1", "New, +3.0 MB on disk"),
            ]
        );
        assert_eq!(view.rows[0].size, "1.5 MB");
        let tiles: Vec<(&str, &str, &str)> = view
            .tiles
            .iter()
            .map(|t| (t.label.as_str(), t.number.as_str(), t.unit.as_str()))
            .collect();
        assert_eq!(
            tiles,
            [
                ("Packages", "2", ""),
                ("Download", "3.0", "MB"),
                ("Extra disk space", "5.0", "MB"),
            ]
        );
        assert_eq!(view.held_title, "Held back by your pacman.conf");
        assert_eq!(view.held_lines, ["linux 1-1 → 2-1, not updated."]);
        assert_eq!(
            view.held_warning,
            "A held package can stop other updates from working."
        );
        assert_eq!(
            view.notes,
            [
                "Nothing will be removed.",
                "Before updating, check Arch news for steps you must do by hand: archlinux.org/news",
            ]
        );
        assert_eq!(labels(&view), (Some("Update 2 packages"), Some("Cancel")));
        assert_eq!(view.hint, "Your desktop asks for your password once.");
        assert_eq!(view.focus, FocusTarget::List);
        assert_eq!(
            view.technical[0],
            "extra/zsh 2-1 · download 1500000 bytes · Installed Size : 2.86 MiB"
        );
        assert_eq!(view.technical.len(), 3);
    }

    #[test]
    fn a_review_with_older_or_changed_lists_says_so() {
        let note = ListsNote {
            why: NotFresh::Failed,
            age_days: Some(2),
        };
        let view = update_view(&after_check(
            UpdateLookup::Plan(Box::new(mixed())),
            Some(note),
        ));
        assert_eq!(
            view.notes.last().map(String::as_str),
            Some("Checked with the package lists updated 2 days ago. Couldn't get newer ones.")
        );

        let mut model = reviewing(mixed());
        let job = model.start_update().unwrap();
        let ended = model.job_ended(
            job.number,
            JobEnd::Finished(Outcome::Error(ErrorKind::TransactionChanged)),
        );
        model.checked(
            ended.check.unwrap().number,
            Ok((UpdateLookup::Plan(Box::new(mixed())), None)),
        );
        assert_eq!(
            update_view(&model).notes.first().map(String::as_str),
            Some("The package lists changed since you looked. Tidewise got the new list.")
        );
    }

    #[test]
    fn up_to_date_still_shows_what_is_held() {
        let mut plan = mixed();
        plan.items.clear();
        let view = update_view(&reviewing(plan));
        assert_eq!(view.kind, UpdateKind::UpToDate);
        assert_eq!(view.title, "Your system is up to date");
        assert_eq!(view.held_lines.len(), 1);
        assert!(view.rows.is_empty());
        assert_eq!(labels(&view), (Some("Check again"), None));
    }

    #[test]
    fn a_replacement_card_names_the_replacement_under_the_details() {
        let mut plan = mixed();
        plan.replacements.push(tidewise_pacman::Replacement {
            new: pkg("new-pkg"),
            old: "old-pkg".to_string(),
        });
        let view = update_view(&reviewing(plan));
        assert_eq!(view.kind, UpdateKind::Blocked);
        assert_eq!(
            view.title,
            "This update includes a replacement Tidewise can't do yet"
        );
        assert_eq!(view.technical, ["extra/new-pkg replaces old-pkg"]);
        assert!(view.rows.is_empty());
        assert_eq!(labels(&view), (Some("Check again"), Some("Close")));
    }

    #[test]
    fn a_blocked_update_says_why_and_offers_no_update() {
        let view = update_view(&after_check(
            UpdateLookup::HeldBlock(HeldBlock {
                package: "a".to_string(),
                held: "b".to_string(),
            }),
            None,
        ));
        assert_eq!(view.kind, UpdateKind::Blocked);
        assert_eq!(view.title, "An update needs a package you've held back");
        assert!(view.body.starts_with("a needs a newer b"));
        assert!(view.advice.is_empty());
        assert_eq!(labels(&view), (Some("Check again"), Some("Close")));
    }

    #[test]
    fn a_failed_check_keeps_pacmans_words_under_the_details() {
        let mut model = UpdateModel::new();
        let ticket = model.check().unwrap();
        model.checked(
            ticket.number,
            Err(PacmanError::Failed {
                code: Some(1),
                stderr: "error: boom".to_string(),
            }),
        );
        let view = update_view(&model);
        assert_eq!(view.kind, UpdateKind::Alert);
        assert_eq!(view.title, "Tidewise couldn't check for updates");
        assert_eq!(view.technical, ["exit code 1: error: boom"]);
        assert_eq!(labels(&view), (Some("Check again"), Some("Close")));
    }

    #[test]
    fn progress_names_the_stage_and_offers_no_button() {
        let mut model = reviewing(mixed());
        let job = model.start_update().unwrap();
        let waiting = update_view(&model);
        assert_eq!(waiting.kind, UpdateKind::Progress);
        assert_eq!(waiting.title, "Waiting for your password");
        assert_eq!(labels(&waiting), (None, None));
        model.job_event(
            job.number,
            Event::Progress(Progress {
                phase: Stage::Installing,
                current: 1,
                total: 3,
            }),
        );
        let view = update_view(&model);
        assert_eq!(view.title, "Updating 1 of 3");
        assert_eq!(view.stages[2].label, "Updating");
        assert_eq!(view.stages[2].state, StageState::Now);
        assert_eq!(view.percent_text, "64 %");
        assert_eq!(
            view.hint,
            "Updating can't be stopped halfway. Keep Tidewise open until it's done."
        );
    }

    #[test]
    fn a_closed_password_window_is_said_on_the_review() {
        let mut model = reviewing(mixed());
        let job = model.start_update().unwrap();
        model.job_ended(job.number, JobEnd::PromptClosed);
        assert_eq!(
            update_view(&model).primary_note,
            "The password prompt was closed. Nothing was changed."
        );
    }

    fn finished(names: &[&str], notices: &[Notice]) -> UpdateView {
        let plan = UpdatePlan {
            items: names.iter().map(|n| item(n, Some("1-1"))).collect(),
            ..UpdatePlan::default()
        };
        let mut model = reviewing(plan);
        let job = model.start_update().unwrap();
        for notice in notices {
            model.job_event(job.number, Event::Notice(*notice));
        }
        model.job_ended(job.number, JobEnd::Finished(Outcome::Ok));
        update_view(&model)
    }

    #[test]
    fn the_done_card_adds_the_restart_line_only_after_a_core_part() {
        let kernel = finished(&["zsh", "linux"], &[]);
        assert_eq!(kernel.kind, UpdateKind::Done);
        assert_eq!(kernel.title, "Your system is up to date");
        assert_eq!(
            kernel.body,
            "A new kernel or core system part was installed. Restart when convenient."
        );
        assert!(!kernel.attention);
        let plain = finished(&["zsh"], &[]);
        assert_eq!(plain.body, "");
        assert_eq!(labels(&plain), (Some("Done"), None));
    }

    #[test]
    fn a_done_card_with_notices_warns_and_claims_no_more() {
        let view = finished(&["linux", "zsh"], &[Notice::PackagesNotInstalled(1)]);
        assert_eq!(view.title, "The update finished");
        assert!(view.attention);
        assert_eq!(
            view.body,
            "1 package from your list was not installed.\nA new kernel or core system part was installed. Restart \
             when convenient."
        );
    }

    fn ended(end: JobEnd) -> UpdateView {
        let mut model = reviewing(mixed());
        let job = model.start_update().unwrap();
        model.job_ended(job.number, end);
        update_view(&model)
    }

    #[test]
    fn a_failed_update_claims_only_what_is_known() {
        let refused = ended(JobEnd::Finished(Outcome::Error(
            ErrorKind::ProviderNotChosen,
        )));
        assert_eq!(refused.kind, UpdateKind::Alert);
        assert_eq!(refused.title, "Tidewise couldn't update your system");
        assert!(refused.body.ends_with("pacman will ask you which one."));
        let broke = ended(JobEnd::Finished(Outcome::Error(ErrorKind::PacmanFailed(
            Details::from_raw("error: x"),
        ))));
        assert_eq!(broke.title, "The update ended with an error");
        assert!(broke.body.ends_with("Check Apps to see what is installed."));
        assert_eq!(broke.technical, ["error: x"]);
        let unknown = ended(JobEnd::NoResult {
            exit: None,
            unreadable: 0,
            stderr: String::new(),
        });
        assert_eq!(unknown.title, "The result of the update is unknown");
        let missing = ended(JobEnd::HelperMissing);
        assert_eq!(
            missing.body,
            "Tidewise's helper is not installed, so Tidewise can't update your system. Reinstall Tidewise. Nothing \
             was changed."
        );
        assert_eq!(labels(&missing), (Some("Check again"), Some("Close")));
    }

    fn ended_with(notices: &[Notice], kind: ErrorKind) -> UpdateView {
        let mut model = reviewing(mixed());
        let job = model.start_update().unwrap();
        for notice in notices {
            model.job_event(job.number, Event::Notice(*notice));
        }
        model.job_ended(job.number, JobEnd::Finished(Outcome::Error(kind)));
        update_view(&model)
    }

    const UNREACHABLE: &str =
        include_str!("../../../tests/fixtures/pacman/container/download-su-unresolvable.stderr");

    #[test]
    fn a_failed_download_says_so_with_pacmans_words_underneath() {
        let view = ended_with(
            &[Notice::PackagesNotInstalled(2)],
            ErrorKind::PacmanFailed(Details::from_raw(UNREACHABLE)),
        );
        assert_eq!(view.title, "Tidewise couldn't update your system");
        assert_eq!(
            view.body,
            "Tidewise couldn't download the packages. Your connection may be slow or a mirror may be down. Nothing \
             was changed."
        );
        assert_eq!(view.technical, UNREACHABLE.lines().collect::<Vec<_>>());
    }

    #[test]
    fn a_failure_that_installed_nothing_says_nothing_was_installed() {
        let view = ended_with(
            &[Notice::PackagesNotInstalled(2)],
            ErrorKind::PacmanFailed(Details::from_raw("error: x")),
        );
        assert!(
            view.body.ends_with("\nNothing was installed."),
            "{}",
            view.body
        );
    }

    #[test]
    fn a_done_card_that_installed_nothing_says_so_and_asks_for_no_restart() {
        let view = finished(&["linux", "zsh"], &[Notice::PackagesNotInstalled(2)]);
        assert_eq!(view.title, "The update finished");
        assert_eq!(view.body, "Nothing was installed.");
    }

    #[test]
    fn a_held_lock_says_another_update_is_still_running() {
        let view = ended_with(&[], ErrorKind::DatabaseLocked);
        assert_eq!(
            view.body,
            "Another update is still running (maybe one Tidewise started earlier). Wait for it to finish, then check \
             again."
        );
    }
}
