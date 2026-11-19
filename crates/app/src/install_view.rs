//! Turns the `InstallModel` into the exact words and flags the install screen shows (WBS 6.2).
//! No Slint types here, so every string the window displays is unit-tested. Words follow the design boards
//! "M2 Install flow" and "M4 Install questions".

use crate::install_model::InstallModel;

/// Which card fills the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardKind {
    #[default]
    Closed,
    /// "Working out what ‹name› needs…"
    Working,
    /// "Your system needs updating first"
    Stale,
    /// Radio list of providers.
    Provider,
    /// Checkbox list of group members.
    Members,
    /// Already installed, same version: leave or reinstall.
    Choice,
    /// Installed version differs from the repository's: left out.
    Differs,
    /// Blocked or failed: amber words.
    Alert,
    /// The placeholder where 6.2 stops.
    Ready,
}

/// One choice in a radio or checkbox list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OptionView {
    pub label: String,
    /// A version, shown small and monospaced next to the label.
    pub meta: String,
    pub detail: String,
    /// "pacman's default", or empty.
    pub tag: String,
    pub checked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ButtonView {
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstalledRow {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstallView {
    pub open: bool,
    /// "Install dbeaver"
    pub heading: String,
    /// What follows "Search ›" in the breadcrumb.
    pub crumb: String,
    /// Which of Choose / Review / Install is current (0 or 1 in 6.2).
    pub current_pill: usize,
    pub kind: CardKind,
    /// "Question 2 of 3", or empty with one question.
    pub step_text: String,
    /// One per question: 0 answered, 1 shown, 2 to come. Empty with one question.
    pub dots: Vec<i32>,
    /// The title in three pieces: the middle one is shown monospaced (a dependency name).
    pub title_before: String,
    pub title_mono: String,
    pub title_after: String,
    pub body: String,
    /// Reads aloud before the options: "Question 2 of 3. …".
    pub group_label: String,
    pub options: Vec<OptionView>,
    /// "5 of 5 selected · 3 already installed"
    pub selection_text: String,
    pub show_select_buttons: bool,
    pub installed_heading: String,
    pub installed_rows: Vec<InstalledRow>,
    /// A monospaced block that explains the card (versions, what was needed).
    pub code_lines: Vec<String>,
    pub primary: Option<ButtonView>,
    pub secondary: Option<ButtonView>,
    /// "Nothing was installed and nothing on your system changed.", or empty.
    pub unchanged: String,
    /// "This is taking longer than usual.", or empty.
    pub slow_note: String,
    /// A line under the buttons (the stale card's terminal hint).
    pub hint: String,
    /// "Report problems at …", or empty.
    pub report: String,
    /// Behind the "Technical details" toggle.
    pub technical: Vec<String>,
    pub focus_serial: u32,
}

pub fn install_view(model: &InstallModel) -> InstallView {
    let _ = model;
    InstallView::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_model::WorkAnswer;
    use tidewise_pacman::{
        Blocker, Candidate, GroupChoice, InstallDecision, InstallPlan, PackageRef, PacmanError,
        PlanItem, Question, Resolution, Role,
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
                    version: "25.0-1".to_string(),
                    description: format!("{name} runtime"),
                })
                .collect(),
            default,
        }
    }

    fn group(members: &[(&str, Option<&str>)]) -> Question {
        Question::ChooseGroupMembers {
            group: "plasma".to_string(),
            members: members
                .iter()
                .map(|(name, installed)| GroupChoice {
                    package: pkg(name),
                    version: "6.5-1".to_string(),
                    installed_version: installed.map(str::to_string),
                })
                .collect(),
        }
    }

    fn installed(name: &str, differs: bool) -> Question {
        Question::AlreadyInstalled {
            target: pkg(name),
            installed_version: "3.0.22-2".to_string(),
            repo_version: if differs { "3.0.21-9" } else { "3.0.22-2" }.to_string(),
            differs_without_update: differs,
        }
    }

    fn plan(items: usize) -> InstallPlan {
        InstallPlan {
            decision: InstallDecision::Allowed { ignored: 0 },
            targets: vec![pkg("dbeaver")],
            providers: Vec::new(),
            items: (0..items)
                .map(|n| PlanItem {
                    package: pkg(&format!("p{n}")),
                    version: "1-1".to_string(),
                    role: Role::Dependency,
                    installed_version: None,
                    download_size: 1,
                    installed_size: 1,
                })
                .collect(),
            download_total: 0,
            installed_total: 0,
        }
    }

    fn answered(target: &str, answer: Resolution) -> InstallModel {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg(target)).unwrap();
        model.done(ticket.number, Ok(WorkAnswer::Resolved(answer)));
        model
    }

    fn asking(target: &str, questions: Vec<Question>) -> InstallModel {
        answered(target, Resolution::NeedsAnswers(questions))
    }

    fn blocked(target: &str, blockers: Vec<Blocker>) -> InstallModel {
        answered(target, Resolution::Blocked(blockers))
    }

    fn button(label: &str, enabled: bool) -> Option<ButtonView> {
        Some(ButtonView {
            label: label.to_string(),
            enabled,
        })
    }

    #[test]
    fn a_closed_model_shows_nothing() {
        let view = install_view(&InstallModel::new());
        assert!(!view.open);
        assert_eq!(view.kind, CardKind::Closed);
    }

    #[test]
    fn the_waiting_card_says_what_is_happening_and_can_be_cancelled() {
        let mut model = InstallModel::new();
        model.open(pkg("dbeaver"));
        let view = install_view(&model);
        assert!(view.open);
        assert_eq!(view.kind, CardKind::Working);
        assert_eq!(view.heading, "Install dbeaver");
        assert_eq!(view.crumb, "dbeaver");
        assert_eq!(view.current_pill, 0);
        assert_eq!(view.title_before, "Working out what dbeaver needs…");
        assert_eq!(
            view.body,
            "Asking pacman for a dry run. Nothing is installed yet."
        );
        assert_eq!(view.slow_note, "");
        assert_eq!(view.primary, None);
        assert_eq!(view.secondary, button("Cancel", true));
        model.mark_slow();
        assert_eq!(
            install_view(&model).slow_note,
            "This is taking longer than usual."
        );
    }

    #[test]
    fn the_stale_card_has_its_button_off() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        model.done(ticket.number, Ok(WorkAnswer::MustUpdate));
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Stale);
        assert_eq!(view.title_before, "Your system needs updating first");
        assert_eq!(
            view.body,
            "Arch only supports adding apps to an up-to-date system. \
             Installing dbeaver alone could break other apps."
        );
        assert_eq!(
            view.primary,
            button("Update system and install dbeaver", false)
        );
        assert_eq!(view.secondary, button("Back to details", true));
        assert_eq!(
            view.hint,
            "Coming soon: Tidewise will update and install in one go. For now, update your \
             system in a terminal with sudo pacman -Syu, then come back."
        );
    }

    #[test]
    fn one_provider_question_has_no_step_counter() {
        let model = asking(
            "dbeaver",
            vec![provider(
                "java-runtime",
                &["jre-openjdk", "jre21-openjdk"],
                0,
            )],
        );
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Provider);
        assert_eq!(view.step_text, "");
        assert!(view.dots.is_empty());
        assert_eq!(view.title_before, "Installing dbeaver needs ");
        assert_eq!(view.title_mono, "java-runtime");
        assert_eq!(view.title_after, ": pick one");
        assert_eq!(
            view.body,
            "Any of them works. If you're not sure, keep the one marked as pacman's default."
        );
        assert_eq!(view.group_label, "Pick a package for java-runtime");
        assert_eq!(
            view.options,
            [
                OptionView {
                    label: "jre-openjdk".to_string(),
                    meta: "25.0-1".to_string(),
                    detail: "jre-openjdk runtime".to_string(),
                    tag: "pacman's default".to_string(),
                    checked: true,
                },
                OptionView {
                    label: "jre21-openjdk".to_string(),
                    meta: "25.0-1".to_string(),
                    detail: "jre21-openjdk runtime".to_string(),
                    tag: String::new(),
                    checked: false,
                },
            ]
        );
        assert_eq!(view.primary, button("Continue with jre-openjdk", true));
        assert_eq!(view.secondary, button("Cancel", true));
    }

    #[test]
    fn the_default_tag_follows_pacmans_default_and_the_button_follows_the_pick() {
        let mut model = asking(
            "dbeaver",
            vec![provider("java-runtime", &["a-jre", "b-jre"], 1)],
        );
        let view = install_view(&model);
        assert_eq!(view.options[1].tag, "pacman's default");
        assert!(view.options[1].checked);
        model.choose(0);
        let view = install_view(&model);
        assert!(view.options[0].checked && !view.options[1].checked);
        assert_eq!(view.primary, button("Continue with a-jre", true));
    }

    #[test]
    fn several_questions_come_one_at_a_time() {
        let mut model = asking(
            "dbeaver",
            vec![
                provider("a", &["a1", "a2"], 0),
                provider("b", &["b1", "b2"], 0),
                provider("c", &["c1", "c2"], 0),
            ],
        );
        let view = install_view(&model);
        assert_eq!(view.step_text, "Question 1 of 3");
        assert_eq!(view.dots, [1, 2, 2]);
        assert_eq!(view.primary, button("Next question", true));
        assert_eq!(view.secondary, button("Cancel", true));
        model.go_on();
        let view = install_view(&model);
        assert_eq!(view.step_text, "Question 2 of 3");
        assert_eq!(view.dots, [0, 1, 2]);
        assert_eq!(view.secondary, button("Back", true));
        assert_eq!(view.group_label, "Question 2 of 3. Pick a package for b");
        model.go_on();
        let view = install_view(&model);
        assert_eq!(view.step_text, "Question 3 of 3");
        assert_eq!(view.primary, button("Continue with c1", true));
    }

    #[test]
    fn a_dependency_with_a_version_stays_in_the_mono_part() {
        let model = asking(
            "dbeaver",
            vec![provider("libfoo.so=1-64", &["foo1", "foo2"], 0)],
        );
        assert_eq!(install_view(&model).title_mono, "libfoo.so=1-64");
    }

    #[test]
    fn the_group_card_sets_installed_members_apart() {
        let model = asking(
            "plasma",
            vec![group(&[
                ("plasma-desktop", None),
                ("kwin", Some("6.5-1")),
                ("kscreen", None),
            ])],
        );
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Members);
        assert_eq!(view.title_before, "plasma is a group of packages");
        assert_eq!(
            view.body,
            "All members you don't have yet are selected; untick any you don't want. \
             Members you already have stay as they are."
        );
        assert_eq!(view.selection_text, "2 of 2 selected · 1 already installed");
        assert!(view.show_select_buttons);
        let names: Vec<&str> = view.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(names, ["plasma-desktop", "kscreen"]);
        assert!(view.options.iter().all(|o| o.checked));
        assert_eq!(
            view.installed_heading,
            "Already installed, not installed again"
        );
        assert_eq!(
            view.installed_rows,
            [InstalledRow {
                name: "kwin".to_string(),
                version: "6.5-1".to_string()
            }]
        );
        assert_eq!(view.primary, button("Continue with 2 packages", true));
    }

    #[test]
    fn unticking_members_changes_the_count_and_zero_turns_the_button_off() {
        let mut model = asking("plasma", vec![group(&[("a", None), ("b", None)])]);
        assert_eq!(
            install_view(&model).body,
            "All are selected; untick any you don't want."
        );
        model.toggle(0);
        let view = install_view(&model);
        assert_eq!(view.selection_text, "1 of 2 selected");
        assert_eq!(view.primary, button("Continue with 1 package", true));
        assert!(!view.options[0].checked && view.options[1].checked);
        model.select_none();
        let view = install_view(&model);
        assert_eq!(view.selection_text, "0 of 2 selected");
        assert_eq!(view.primary, button("Continue with 0 packages", false));
    }

    #[test]
    fn a_group_with_every_member_installed_says_so_and_has_only_back() {
        let model = asking(
            "plasma",
            vec![group(&[("kwin", Some("6.5-1")), ("kate", Some("6.5-1"))])],
        );
        let view = install_view(&model);
        assert_eq!(
            view.body,
            "Every member of plasma is already installed, so there is nothing to add."
        );
        assert!(!view.show_select_buttons);
        assert!(view.options.is_empty());
        assert_eq!(view.installed_rows.len(), 2);
        assert_eq!(view.primary, None);
        assert_eq!(view.secondary, button("Cancel", true));
    }

    #[test]
    fn the_same_version_question_offers_leave_and_reinstall() {
        let mut model = asking("zsh", vec![installed("zsh", false)]);
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Choice);
        assert_eq!(view.title_before, "zsh is already installed");
        assert_eq!(
            view.body,
            "You have 3.0.22-2, the same version the repository has. There's usually nothing to do."
        );
        assert_eq!(view.group_label, "What to do with zsh");
        assert_eq!(view.options[0].label, "Leave it as it is");
        assert_eq!(
            view.options[0].detail,
            "Recommended. zsh is not in the list."
        );
        assert!(view.options[0].checked);
        assert_eq!(view.options[1].label, "Reinstall it");
        assert_eq!(
            view.options[1].detail,
            "Writes the same version's files again. Useful if they were damaged."
        );
        assert_eq!(view.primary, button("Continue", true));
        model.choose(1);
        assert!(install_view(&model).options[1].checked);
    }

    #[test]
    fn a_different_version_is_left_out_in_neutral_words() {
        let model = asking(
            "dbeaver",
            vec![installed("vlc", true), provider("x", &["x1", "x2"], 0)],
        );
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Differs);
        assert_eq!(
            view.title_before,
            "Your vlc is a different version from the repository's"
        );
        assert_eq!(
            view.body,
            "You have 3.0.22-2; the repository has 3.0.21-9. Installing it would replace \
             yours, so Tidewise leaves vlc out."
        );
        assert_eq!(
            view.code_lines,
            [
                "installed  vlc 3.0.22-2".to_string(),
                "extra      vlc 3.0.21-9".to_string()
            ]
        );
        assert_eq!(view.primary, button("Continue without vlc", true));
    }

    #[test]
    fn the_only_package_of_another_version_has_only_back() {
        let model = asking("vlc", vec![installed("vlc", true)]);
        let view = install_view(&model);
        assert_eq!(view.primary, None);
        assert_eq!(view.secondary, button("Cancel", true));
    }

    #[test]
    fn a_renamed_package_offers_the_replacement() {
        let model = blocked(
            "cron",
            vec![Blocker::ReplacedBy {
                requested: pkg("cron"),
                by: vec![pkg("cronie")],
            }],
        );
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Alert);
        assert_eq!(view.title_before, "cron is now provided by cronie");
        assert_eq!(
            view.body,
            "You picked extra/cron, but pacman would install extra/cronie for it. \
             Tidewise only installs what you see, so it stopped here."
        );
        assert_eq!(view.primary, button("Install cronie instead", true));
        assert_eq!(view.secondary, button("Back to details", true));
        assert_eq!(view.unchanged, "");
    }

    #[test]
    fn several_replacements_are_listed_and_none_is_offered() {
        let model = blocked(
            "cron",
            vec![Blocker::ReplacedBy {
                requested: pkg("cron"),
                by: vec![pkg("cronie"), pkg("fcron")],
            }],
        );
        let view = install_view(&model);
        assert_eq!(
            view.title_before,
            "cron is now provided by cronie and fcron"
        );
        assert_eq!(view.primary, None);
    }

    #[test]
    fn no_provider_that_fits_is_a_dead_end_with_what_was_needed() {
        let model = blocked(
            "dbeaver",
            vec![Blocker::NothingProvides {
                dependency: "java-runtime>=21".to_string(),
                required_by: "dbeaver".to_string(),
            }],
        );
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Alert);
        assert_eq!(
            view.title_before,
            "Nothing in the repositories gives dbeaver the java-runtime>=21 it needs"
        );
        assert_eq!(
            view.body,
            "dbeaver needs java-runtime>=21, and none of the packages in the repositories fits. \
             This usually sorts itself out when the repositories update."
        );
        assert_eq!(
            view.code_lines,
            ["needed:   java-runtime>=21   (by dbeaver)"]
        );
        assert_eq!(view.primary, None);
        assert_eq!(
            view.unchanged,
            "Nothing was installed and nothing on your system changed."
        );
    }

    #[test]
    fn names_pacman_does_not_know_say_the_lists_changed() {
        let model = blocked(
            "dbeaver",
            vec![Blocker::NotFound(vec!["dbeaver".to_string()])],
        );
        let view = install_view(&model);
        assert_eq!(view.title_before, "pacman doesn't know dbeaver");
        assert_eq!(
            view.body,
            "The package lists may have changed since you searched. \
             Search again to see what is there now."
        );
        assert_eq!(view.technical, ["not found: dbeaver"]);
    }

    #[test]
    fn nothing_to_install_after_leaving_everything_out() {
        let model = blocked("zsh", vec![Blocker::NothingToInstall]);
        let view = install_view(&model);
        assert_eq!(view.title_before, "Nothing to install");
        assert_eq!(view.body, "Everything you chose stays as it is.");
    }

    #[test]
    fn a_target_missing_from_pacmans_list_is_a_bug_card() {
        let model = blocked(
            "dbeaver",
            vec![Blocker::MissingFromList {
                requested: pkg("dbeaver"),
            }],
        );
        let view = install_view(&model);
        assert_eq!(
            view.title_before,
            "Tidewise couldn't work out what to install"
        );
        assert_eq!(
            view.body,
            "pacman's answers didn't add up, so Tidewise did not go on. \
             This is a bug in Tidewise, not something you did."
        );
        assert_eq!(
            view.report,
            "Report problems at github.com/Cyber-Programmer/tidewise/issues"
        );
        assert_eq!(
            view.technical,
            ["missing from pacman's list: extra/dbeaver"]
        );
    }

    #[test]
    fn a_failed_resolve_offers_try_again_unless_it_is_a_bug() {
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        model.done(
            ticket.number,
            Err(PacmanError::Failed {
                code: Some(1),
                stderr: "error: unable to lock database".to_string(),
            }),
        );
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Alert);
        assert_eq!(
            view.title_before,
            "pacman couldn't work out what to install"
        );
        assert_eq!(view.primary, button("Try again", true));
        assert_eq!(
            view.technical,
            ["exit code 1: error: unable to lock database"]
        );
        let ticket = model.try_again().unwrap();
        model.done(ticket.number, Err(PacmanError::TooManyRounds { limit: 5 }));
        let view = install_view(&model);
        assert_eq!(view.title_before, "This install needs too many questions");
        assert_eq!(view.primary, None);
    }

    #[test]
    fn the_ready_placeholder_counts_the_packages_and_stops_there() {
        let model = answered("dbeaver", Resolution::Ready(plan(5)));
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Ready);
        assert_eq!(view.current_pill, 1);
        assert_eq!(view.title_before, "The list is ready");
        assert_eq!(
            view.body,
            "Tidewise knows exactly what it would install: 5 packages. \
             The review screen comes next."
        );
        assert_eq!(view.primary, None);
        assert_eq!(view.secondary, button("Back to details", true));
        let one = install_view(&answered("dbeaver", Resolution::Ready(plan(1))));
        assert!(one.body.contains(": 1 package. "));
    }

    #[test]
    fn the_heading_names_a_group_or_counts_packages() {
        let mut model = InstallModel::new();
        model.open_group("plasma");
        assert_eq!(install_view(&model).heading, "Install plasma");
    }

    #[test]
    fn every_new_card_carries_a_new_focus_number() {
        let mut model = asking(
            "dbeaver",
            vec![
                provider("a", &["a1", "a2"], 0),
                provider("b", &["b1", "b2"], 0),
            ],
        );
        let before = install_view(&model).focus_serial;
        model.go_on();
        assert_ne!(before, install_view(&model).focus_serial);
    }
}
