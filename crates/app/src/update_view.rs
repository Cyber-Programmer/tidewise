//! Turns the `UpdateModel` into the exact words and flags the update screen shows (WBS 6.5). No Slint types here, so
//! every string the window displays is unit-tested. Blocked-card, skipped, held and refusal words are the human's
//! (board M4UpdateSystem).

use crate::install_view::{ButtonView, FocusTarget, ReviewRow, StageView, TileView};
use crate::update_model::UpdateModel;

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
    /// "<new> replaces <old>. …", one per replacement.
    pub replacement_lines: Vec<String>,
    /// Empty when nothing is held.
    pub held_title: String,
    pub held_lines: Vec<String>,
    pub held_warning: String,
    /// "Nothing will be removed.", the news line, and what is said about older or changed lists.
    pub notes: Vec<String>,
    /// Under the button: the password window may count more.
    pub password_line: String,
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

pub fn update_view(_model: &UpdateModel) -> UpdateView {
    UpdateView::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper_run::JobEnd;
    use crate::install_view::StageState;
    use crate::update_check::ListsNote;
    use tidewise_helper_protocol::{
        Details, ErrorKind, Event, Notice, Outcome, Phase as Stage, Progress,
    };
    use tidewise_pacman::{
        HeldBlock, NotFresh, PackageRef, PacmanError, PendingUpdate, PlanItem, Replacement, Role,
        UpdateLookup, UpdatePlan,
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
            items: vec![
                item("zsh", Some("1-1")),
                item("libnew", None),
                item("new-pkg", None),
            ],
            skipped: vec![pkg("new-pkg")],
            replacements: vec![Replacement {
                new: pkg("new-pkg"),
                old: "old-pkg".to_string(),
            }],
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
    fn the_review_lists_what_is_installed_and_says_what_is_not() {
        let view = update_view(&reviewing(mixed()));
        assert_eq!(view.kind, UpdateKind::Review);
        assert_eq!(view.title, "Review the update");
        // The skipped replacement is not a row and is not counted.
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
        assert_eq!(
            view.replacement_lines,
            [
                "new-pkg replaces old-pkg. Tidewise doesn't do replacements yet; run sudo pacman -Syu in a terminal \\
              to accept it."
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
        assert_eq!(
            view.password_line,
            "The password window may count 1 more package: new-pkg, which pacman lists but Tidewise skips."
        );
        assert_eq!(labels(&view), (Some("Update 2 packages"), Some("Cancel")));
        assert_eq!(view.hint, "Your desktop asks for your password once.");
        assert_eq!(view.focus, FocusTarget::List);
        assert_eq!(
            view.technical[0],
            "extra/zsh 2-1 · download 1500000 bytes · Installed Size : 2.86 MiB"
        );
        assert_eq!(view.technical.len(), 4);
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
    fn up_to_date_still_shows_the_replacement_and_what_is_held() {
        let mut plan = mixed();
        plan.items.retain(|i| i.package.name == "new-pkg");
        let view = update_view(&reviewing(plan));
        assert_eq!(view.kind, UpdateKind::UpToDate);
        assert_eq!(view.title, "Your system is up to date");
        assert_eq!(view.replacement_lines.len(), 1);
        assert_eq!(view.held_lines.len(), 1);
        assert!(view.rows.is_empty() && view.password_line.is_empty());
        assert_eq!(labels(&view), (Some("Check again"), None));
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
        let view = finished(&["linux"], &[Notice::PackagesNotInstalled(1)]);
        assert_eq!(view.title, "The update finished");
        assert!(view.attention);
        assert_eq!(
            view.body,
            "1 package from your list was not installed.\\nA new kernel or core system part was installed. Restart \\
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
            "Tidewise's helper is not installed, so Tidewise can't update your system. Reinstall Tidewise. Nothing \\
             was changed."
        );
        assert_eq!(labels(&missing), (Some("Check again"), Some("Close")));
    }
}
