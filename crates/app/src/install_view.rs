//! Turns the `InstallModel` into the exact words and flags the install screen shows (WBS 6.2).
//! No Slint types here, so every string the window displays is unit-tested. Words follow the design boards
//! "M2 Install flow" and "M4 Install questions".

use crate::error_words::{Doing, error_text, error_view};
use crate::install_model::{Draft, InstallModel, Phase, Step};
use crate::size_text::{format_size, split_size};
use crate::update_check::ListsNote;
use tidewise_pacman::{
    Blocker, InstallPlan, NotFresh, PackageRef, PacmanError, PlanItem, Question, Role,
};

const UNCHANGED: &str = "Nothing was installed and nothing on your system changed.";
const REPORT: &str = "Report problems at github.com/Cyber-Programmer/tidewise/issues";

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
    /// The review: the exact list pacman would install (WBS 6.3).
    Review,
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
    /// For a group member: its position among ALL the group's members (installed ones included, which have no row).
    /// The model's `toggle` takes this number, not the row's.
    pub member: Option<usize>,
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

/// One of the three figures at the top of the review.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TileView {
    pub label: String,
    pub number: String,
    pub unit: String,
    /// `number` is a sentence, not a figure: it is drawn smaller and may wrap.
    pub words: bool,
}

/// One package of the review list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewRow {
    pub name: String,
    pub version: String,
    /// Why it is in the list.
    pub why: String,
    pub size: String,
    /// The user asked for it or picked it.
    pub bold: bool,
}

/// Which element takes the keyboard focus when a card appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusTarget {
    /// The radio group, or the first checkbox.
    List,
    #[default]
    Primary,
    Secondary,
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
    /// Said when the updates check could only use older lists, or empty.
    pub lists_note: String,
    /// "Report problems at …", or empty.
    pub report: String,
    /// The review's three figures.
    pub tiles: Vec<TileView>,
    /// The review's list, in pacman's order.
    pub rows: Vec<ReviewRow>,
    /// A line under the buttons that says why the primary one is off, or empty.
    pub primary_note: String,
    /// Behind the "Technical details" toggle.
    pub technical: Vec<String>,
    pub focus_serial: u32,
    pub focus: FocusTarget,
}

pub fn install_view(model: &InstallModel) -> InstallView {
    if !model.is_open() {
        return InstallView::default();
    }
    let label = request_label(model);
    let mut view = InstallView {
        open: true,
        heading: format!("Install {label}"),
        crumb: label.clone(),
        focus_serial: model.focus_serial(),
        ..InstallView::default()
    };
    match model.phase() {
        Phase::Closed => {}
        Phase::Working => {
            view.kind = CardKind::Working;
            view.title_before = format!("Working out what {label} needs…");
            view.body = "Asking pacman for a dry run. Nothing is installed yet.".to_string();
            if model.is_slow() {
                view.slow_note = "This is taking longer than usual.".to_string();
            }
            view.secondary = button("Cancel", true);
        }
        Phase::Asking => {
            if let Some(step) = model.steps().get(model.current()) {
                asking(&mut view, model, step, &label);
            }
        }
        Phase::Stale => {
            view.kind = CardKind::Stale;
            view.title_before = "Your system needs updating first".to_string();
            view.body = format!(
                "Arch only supports adding apps to an up-to-date system. \
                 Installing {label} alone could break other apps."
            );
            view.primary = button(&format!("Update system and install {label}"), false);
            view.secondary = button(back_label(model), true);
            view.hint = "Coming soon: Tidewise will update and install in one go. For now, \
                         update your system in a terminal with sudo pacman -Syu, then come back."
                .to_string();
        }
        Phase::Blocked(blockers) => blocked(&mut view, model, blockers),
        Phase::Failed { text, can_retry } => {
            view.kind = CardKind::Alert;
            view.title_before.clone_from(&text.title);
            view.body.clone_from(&text.body);
            view.technical = vec![text.technical.clone()];
            view.unchanged = UNCHANGED.to_string();
            view.report = error_view(text).report;
            if *can_retry {
                view.primary = button("Try again", true);
            }
            view.secondary = button(back_label(model), true);
        }
        Phase::Ready(plan) => review(&mut view, model, plan, &label),
    }
    view.lists_note = model.lists_note().map(lists_note).unwrap_or_default();
    view.focus = match view.kind {
        CardKind::Provider | CardKind::Choice | CardKind::Review => FocusTarget::List,
        CardKind::Members if !view.options.is_empty() => FocusTarget::List,
        _ => match &view.primary {
            Some(primary) if primary.enabled => FocusTarget::Primary,
            _ => FocusTarget::Secondary,
        },
    };
    view
}

/// How old the lists are that the updates check had to use, and why no newer ones. No `fakeroot` (or no session
/// folder) is not a problem to report: only the age is said.
fn lists_note(note: ListsNote) -> String {
    let age = match note.age_days {
        None => "Checked with the package lists already on this computer.".to_string(),
        Some(0) => "Checked with the package lists updated today.".to_string(),
        Some(1) => "Checked with the package lists updated 1 day ago.".to_string(),
        Some(days) => format!("Checked with the package lists updated {days} days ago."),
    };
    match note.why {
        NotFresh::Unavailable => age,
        NotFresh::Busy => format!("{age} Another refresh of the package lists is still running."),
        NotFresh::Failed => format!("{age} Couldn't get newer ones."),
    }
}

fn button(label: &str, enabled: bool) -> Option<ButtonView> {
    Some(ButtonView {
        label: label.to_string(),
        enabled,
    })
}

/// "1 package", "5 packages".
fn packages(count: usize) -> String {
    if count == 1 {
        "1 package".to_string()
    } else {
        format!("{count} packages")
    }
}

/// "a", "a and b", "a, b and c".
fn names_text(names: &[String]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

fn spec(item: &PackageRef) -> String {
    format!("{}/{}", item.repo, item.name)
}

/// What the screen calls the thing being installed.
fn request_label(model: &InstallModel) -> String {
    let request = model.request();
    match (request.targets.as_slice(), request.groups.as_slice()) {
        ([only], []) => only.name.clone(),
        ([], [only]) => only.clone(),
        _ => packages(request.targets.len() + request.groups.len()),
    }
}

/// Where the secondary button of a result card goes: back to the last question, or out to the details.
fn back_label(model: &InstallModel) -> &'static str {
    if model.steps().is_empty() {
        "Back to details"
    } else {
        "Back"
    }
}

fn asking(view: &mut InstallView, model: &InstallModel, step: &Step, label: &str) {
    let total = model.steps().len();
    let current = model.current();
    let last = current + 1 == total;
    let prefix = if total > 1 {
        view.step_text = format!("Question {} of {total}", current + 1);
        view.dots = (0..total)
            .map(|index| match index.cmp(&current) {
                std::cmp::Ordering::Less => 0,
                std::cmp::Ordering::Equal => 1,
                std::cmp::Ordering::Greater => 2,
            })
            .collect();
        format!("{}. ", view.step_text)
    } else {
        String::new()
    };
    view.secondary = button(if current == 0 { "Cancel" } else { "Back" }, true);
    let next = |text: String| button(if last { &text } else { "Next question" }, true);
    match (&step.question, &step.draft) {
        (
            Question::ChooseProvider {
                dependency,
                candidates,
                default,
            },
            Draft::Provider(picked),
        ) => {
            view.kind = CardKind::Provider;
            view.title_before = format!("Installing {label} needs ");
            view.title_mono.clone_from(dependency);
            view.title_after = ": pick one".to_string();
            view.body = "Any of them works. If you're not sure, keep the one marked as pacman's \
                         default."
                .to_string();
            view.group_label = format!("{prefix}Pick a package for {dependency}");
            view.options = candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| OptionView {
                    label: candidate.package.name.clone(),
                    meta: candidate.version.clone(),
                    detail: candidate.description.clone(),
                    tag: if index == *default {
                        "pacman's default".to_string()
                    } else {
                        String::new()
                    },
                    checked: index == *picked,
                    member: None,
                })
                .collect();
            let name = candidates
                .get(*picked)
                .map_or("this one", |candidate| candidate.package.name.as_str());
            view.primary = next(format!("Continue with {name}"));
        }
        (Question::ChooseGroupMembers { group, members }, Draft::Members(ticked)) => {
            view.kind = CardKind::Members;
            view.title_before = format!("{group} is a group of packages");
            view.group_label = format!("{prefix}Members to install");
            let missing = members
                .iter()
                .filter(|member| member.installed_version.is_none())
                .count();
            let have = members.len() - missing;
            for (position, (member, on)) in members.iter().zip(ticked).enumerate() {
                match &member.installed_version {
                    None => view.options.push(OptionView {
                        label: member.package.name.clone(),
                        meta: member.version.clone(),
                        checked: *on,
                        member: Some(position),
                        ..OptionView::default()
                    }),
                    Some(version) => view.installed_rows.push(InstalledRow {
                        name: member.package.name.clone(),
                        version: version.clone(),
                    }),
                }
            }
            if have > 0 {
                view.installed_heading = "Already installed, not installed again".to_string();
            }
            if missing == 0 {
                view.body = format!(
                    "Every member of {group} is already installed, so there is nothing to add."
                );
            } else {
                let count = ticked.iter().filter(|on| **on).count();
                view.body = if have > 0 {
                    "All members you don't have yet are selected; untick any you don't want. \
                     Members you already have stay as they are."
                } else {
                    "All are selected; untick any you don't want."
                }
                .to_string();
                view.selection_text = format!("{count} of {missing} selected");
                if have > 0 {
                    view.selection_text
                        .push_str(&format!(" · {have} already installed"));
                }
                view.show_select_buttons = true;
                view.primary = button(
                    &if last {
                        format!("Continue with {}", packages(count))
                    } else {
                        "Next question".to_string()
                    },
                    count > 0,
                );
            }
        }
        (
            Question::AlreadyInstalled {
                target,
                installed_version,
                repo_version,
                ..
            },
            Draft::Differs,
        ) => {
            view.kind = CardKind::Differs;
            let name = &target.name;
            view.title_before = format!("Your {name} is a different version from the repository's");
            view.body = format!(
                "You have {installed_version}; the repository has {repo_version}. \
                 Installing it would replace yours, so Tidewise leaves {name} out."
            );
            view.code_lines = vec![
                format!("{:<10} {name} {installed_version}", "installed"),
                format!("{:<10} {name} {repo_version}", target.repo),
            ];
            if model.can_go_on() {
                view.primary = button(&format!("Continue without {name}"), true);
            }
        }
        (
            Question::AlreadyInstalled {
                target,
                installed_version,
                ..
            },
            Draft::Installed { reinstall },
        ) => {
            view.kind = CardKind::Choice;
            let name = &target.name;
            view.title_before = format!("{name} is already installed");
            view.body = format!(
                "You have {installed_version}, the same version the repository has. \
                 There's usually nothing to do."
            );
            view.group_label = format!("{prefix}What to do with {name}");
            view.options = vec![
                OptionView {
                    label: "Leave it as it is".to_string(),
                    detail: format!("Recommended. {name} is not in the list."),
                    checked: !reinstall,
                    ..OptionView::default()
                },
                OptionView {
                    label: "Reinstall it".to_string(),
                    detail: "Writes the same version's files again. Useful if they were damaged."
                        .to_string(),
                    checked: *reinstall,
                    ..OptionView::default()
                },
            ];
            view.primary = next("Continue".to_string());
        }
        // A draft that does not belong to its question cannot happen; show nothing rather than guess.
        _ => {}
    }
}

/// The review: what `plan` would install, why each package is there, and what it costs.
fn review(view: &mut InstallView, model: &InstallModel, plan: &InstallPlan, label: &str) {
    view.kind = CardKind::Review;
    view.current_pill = 1;
    view.title_before = format!("Review: install {label}");
    view.body = "Nothing will be removed. This is the exact list pacman would install.".to_string();
    view.rows = plan
        .items
        .iter()
        .map(|item| review_row(model, item))
        .collect();
    let disk = disk_tile(&plan.items);
    view.technical = plan.items.iter().map(technical_item).collect();
    view.technical.push(format!(
        "The {} total is added up from pacman's rounded figures.",
        disk.label
    ));
    view.technical
        .push("pacman counts in MiB (1 MiB = 1,048,576 bytes)".to_string());
    view.tiles = vec![
        TileView {
            label: "Packages".to_string(),
            number: plan.items.len().to_string(),
            ..TileView::default()
        },
        download_tile(plan.download_total),
        disk,
    ];
    // Installing arrives with WBS 6.4.
    view.primary = button(&format!("Install {}", packages(plan.items.len())), false);
    view.primary_note =
        "Tidewise can't install yet: this step comes in a later version.".to_string();
    view.secondary = button(back_label(model), true);
    view.hint = "Your desktop asks for your password once.".to_string();
}

fn review_row(model: &InstallModel, item: &PlanItem) -> ReviewRow {
    let asked = model.request().targets.contains(&item.package);
    // A group's kept members are targets nobody named one by one; a provider counts only if the user was asked.
    let picked = match item.role {
        Role::Target => !asked,
        Role::Provider => was_picked(model, &item.package),
        // `Update` cannot reach the review yet: such a plan shows "needs updating first" (6.6 gives it a row).
        Role::Dependency | Role::Update => false,
    };
    let why = if asked {
        "You asked for this"
    } else if picked {
        "You picked this"
    } else {
        "Needed by what you're installing"
    };
    ReviewRow {
        name: item.package.name.clone(),
        version: item.version.clone(),
        why: why.to_string(),
        // `%s` is what is still to download: 0 for a file that is in the cache already.
        size: if item.download_size == 0 {
            "Already downloaded".to_string()
        } else {
            format_size(item.download_size)
        },
        bold: asked || picked,
    }
}

/// Was this package the answer to a provider question?
fn was_picked(model: &InstallModel, package: &PackageRef) -> bool {
    model
        .steps()
        .iter()
        .any(|step| match (&step.question, &step.draft) {
            (Question::ChooseProvider { candidates, .. }, Draft::Provider(index)) => candidates
                .get(*index)
                .is_some_and(|candidate| candidate.package == *package),
            _ => false,
        })
}

fn size_tile(label: &str, bytes: u64) -> TileView {
    let (number, unit) = split_size(&format_size(bytes));
    TileView {
        label: label.to_string(),
        number,
        unit,
        words: false,
    }
}

fn download_tile(bytes: u64) -> TileView {
    if bytes == 0 {
        TileView {
            label: "Download".to_string(),
            number: "Nothing to download".to_string(),
            words: true,
            ..TileView::default()
        }
    } else {
        size_tile("Download", bytes)
    }
}

/// New installed sizes less what the installed versions take now (pacman's "Net Upgrade Size").
fn disk_tile(items: &[PlanItem]) -> TileView {
    let new = items
        .iter()
        .fold(0u64, |sum, item| sum.saturating_add(item.installed_size));
    let now = items.iter().fold(0u64, |sum, item| {
        sum.saturating_add(item.current_size.unwrap_or(0))
    });
    let label = if new < now {
        "Disk space freed"
    } else {
        "Extra disk space"
    };
    size_tile(label, new.abs_diff(now))
}

/// pacman's own figures for one package.
fn technical_item(item: &PlanItem) -> String {
    let installed = if item.installed_size_text.is_empty() {
        "unknown"
    } else {
        &item.installed_size_text
    };
    format!(
        "{} {} · download {} bytes · Installed Size : {installed}",
        spec(&item.package),
        item.version,
        item.download_size
    )
}

fn blocked(view: &mut InstallView, model: &InstallModel, blockers: &[Blocker]) {
    view.kind = CardKind::Alert;
    view.unchanged = UNCHANGED.to_string();
    view.secondary = button(back_label(model), true);
    view.technical = blockers.iter().map(technical_line).collect();
    match blockers.first() {
        Some(Blocker::ReplacedBy { requested, by }) => {
            let names: Vec<String> = by.iter().map(|item| item.name.clone()).collect();
            let specs: Vec<String> = by.iter().map(spec).collect();
            view.title_before = format!(
                "{} is now provided by {}",
                requested.name,
                names_text(&names)
            );
            view.body = format!(
                "You picked {}, but pacman would install {} for it. \
                 Tidewise only installs what you see, so it stopped here.",
                spec(requested),
                names_text(&specs)
            );
            view.unchanged = String::new();
            if let ([_], [only]) = (blockers, by.as_slice()) {
                view.primary = button(&format!("Install {} instead", only.name), true);
            }
        }
        Some(Blocker::NothingProvides {
            dependency,
            required_by,
        }) => {
            view.title_before = format!(
                "Nothing in the repositories gives {required_by} the {dependency} it needs"
            );
            view.body = format!(
                "{required_by} needs {dependency}, and none of the packages in the repositories \
                 fits. This usually sorts itself out when the repositories update."
            );
            view.code_lines = vec![format!("needed:   {dependency}   (by {required_by})")];
        }
        Some(Blocker::NotFound(names)) => {
            view.title_before = format!("pacman doesn't know {}", names_text(names));
            view.body = unknown_body();
        }
        Some(Blocker::UnknownRepo(repo)) => {
            view.title_before = format!("pacman doesn't know the repository {repo}");
            view.body = unknown_body();
        }
        Some(Blocker::UnknownGroup(group)) => {
            view.title_before = format!("pacman doesn't know the group {group}");
            view.body = unknown_body();
        }
        Some(Blocker::MissingFromList { .. }) => {
            // The same words as any other "pacman's answers did not add up" stop.
            let text = error_text(
                &PacmanError::Inconsistent {
                    reason: "a requested package is missing from the list",
                },
                Doing::Resolving,
            );
            view.title_before = text.title;
            view.body = text.body;
            view.report = REPORT.to_string();
        }
        Some(Blocker::TwoVersionsNeeded { .. }) => {}
        Some(Blocker::NothingToInstall) | None => {
            view.title_before = "Nothing to install".to_string();
            view.body = "Everything you chose stays as it is.".to_string();
        }
    }
}

fn unknown_body() -> String {
    "The package lists may have changed since you searched. \
     Search again to see what is there now."
        .to_string()
}

/// One line of the technical details for a blocker.
fn technical_line(blocker: &Blocker) -> String {
    match blocker {
        Blocker::NothingToInstall => "nothing to install".to_string(),
        Blocker::NotFound(names) => format!("not found: {}", names.join(", ")),
        Blocker::UnknownRepo(repo) => format!("unknown repository: {repo}"),
        Blocker::UnknownGroup(group) => format!("unknown group: {group}"),
        Blocker::NothingProvides {
            dependency,
            required_by,
        } => format!("{required_by} needs {dependency}: nothing provides it"),
        Blocker::ReplacedBy { requested, by } => format!(
            "{} is provided by {}",
            spec(requested),
            by.iter().map(spec).collect::<Vec<_>>().join(", ")
        ),
        Blocker::MissingFromList { requested } => {
            format!("missing from pacman's list: {}", spec(requested))
        }
        Blocker::TwoVersionsNeeded { .. } => String::new(),
    }
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
                    installed_size_text: String::new(),
                    current_size: None,
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
                    member: None,
                },
                OptionView {
                    label: "jre21-openjdk".to_string(),
                    meta: "25.0-1".to_string(),
                    detail: "jre21-openjdk runtime".to_string(),
                    tag: String::new(),
                    checked: false,
                    member: None,
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
    fn a_member_row_knows_its_place_among_all_members_so_the_right_box_is_ticked() {
        let mut model = asking(
            "plasma",
            vec![group(&[
                ("kwin", Some("6.5-1")),
                ("kate", Some("6.5-1")),
                ("plasma-desktop", None),
                ("kscreen", None),
            ])],
        );
        let view = install_view(&model);
        let members: Vec<Option<usize>> = view.options.iter().map(|o| o.member).collect();
        // two rows, for members 2 and 3 (0 and 1 are installed and have no row)
        assert_eq!(members, [Some(2), Some(3)]);
        // the window toggles by that number: row 1 (kscreen) goes off, row 0 stays on
        model.toggle(view.options[1].member.unwrap());
        let view = install_view(&model);
        assert!(view.options[0].checked);
        assert!(!view.options[1].checked);
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
    fn two_versions_of_one_name_say_so_and_offer_no_way_on() {
        let model = blocked(
            "need8",
            vec![Blocker::TwoVersionsNeeded {
                dependency: "jv".to_string(),
            }],
        );
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Alert);
        assert_eq!(
            view.title_before,
            "These packages need two different versions of jv; Tidewise can't install them together yet."
        );
        assert_eq!(view.body, "");
        assert_eq!(view.primary, None);
        assert_eq!(view.technical, ["jv: needed at two versions"]);
        assert_eq!(
            view.unchanged,
            "Nothing was installed and nothing on your system changed."
        );
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

    // ---- the review (WBS 6.3) ----------------------------------------------------------------------------

    /// A package of a plan: `download` is pacman's `%s`, `current` what the installed version takes now.
    fn item(name: &str, role: Role, download: u64, size: u64, current: Option<u64>) -> PlanItem {
        PlanItem {
            package: pkg(name),
            version: "1.0-1".to_string(),
            role,
            installed_version: current.map(|_| "1.0-1".to_string()),
            download_size: download,
            installed_size: size,
            installed_size_text: format!("{name} KiB"),
            current_size: current,
        }
    }

    fn plan_of(items: Vec<PlanItem>) -> InstallPlan {
        InstallPlan {
            decision: InstallDecision::Allowed { ignored: 0 },
            targets: vec![pkg("dbeaver")],
            providers: Vec::new(),
            download_total: items.iter().map(|i| i.download_size).sum(),
            installed_total: items.iter().map(|i| i.installed_size).sum(),
            items,
        }
    }

    fn reviewing(items: Vec<PlanItem>) -> InstallView {
        install_view(&answered("dbeaver", Resolution::Ready(plan_of(items))))
    }

    fn tile(label: &str, number: &str, unit: &str) -> TileView {
        TileView {
            label: label.to_string(),
            number: number.to_string(),
            unit: unit.to_string(),
            words: false,
        }
    }

    fn row(name: &str, why: &str, size: &str, bold: bool) -> ReviewRow {
        ReviewRow {
            name: name.to_string(),
            version: "1.0-1".to_string(),
            why: why.to_string(),
            size: size.to_string(),
            bold,
        }
    }

    /// A dbeaver install after one provider question, where `jre21-openjdk` was picked.
    fn picked_a_provider() -> InstallModel {
        let mut model = asking(
            "dbeaver",
            vec![provider(
                "java-runtime",
                &["jre-openjdk", "jre21-openjdk"],
                0,
            )],
        );
        model.choose(1);
        let ticket = model.go_on().expect("the last question starts a resolve");
        let plan = plan_of(vec![
            item("java-common", Role::Provider, 2_000, 4_000, None),
            item("jre21-openjdk", Role::Provider, 3_000_000, 5_000_000, None),
            item("nss", Role::Dependency, 0, 1_000_000, None),
            item("dbeaver", Role::Target, 100_000_000, 128_400_000, None),
        ]);
        model.done(
            ticket.number,
            Ok(WorkAnswer::Resolved(Resolution::Ready(plan))),
        );
        model
    }

    #[test]
    fn the_review_says_what_it_is_and_has_its_install_button_off() {
        let view = install_view(&picked_a_provider());
        assert_eq!(view.kind, CardKind::Review);
        assert_eq!(view.current_pill, 1);
        assert_eq!(view.title_before, "Review: install dbeaver");
        assert_eq!(
            view.body,
            "Nothing will be removed. This is the exact list pacman would install."
        );
        assert_eq!(view.primary, button("Install 4 packages", false));
        assert_eq!(
            view.primary_note,
            "Tidewise can't install yet: this step comes in a later version."
        );
        assert_eq!(view.hint, "Your desktop asks for your password once.");
        assert_eq!(view.secondary, button("Back", true));
        assert_eq!(view.focus, FocusTarget::List);
        assert_eq!(view.unchanged, "");
    }

    #[test]
    fn one_package_is_counted_in_the_singular_and_with_no_question_back_goes_to_details() {
        let view = reviewing(vec![item("dbeaver", Role::Target, 10, 20, None)]);
        assert_eq!(view.primary, button("Install 1 package", false));
        assert_eq!(view.secondary, button("Back to details", true));
    }

    #[test]
    fn each_row_says_why_it_is_there_in_pacmans_order() {
        let view = install_view(&picked_a_provider());
        assert_eq!(
            view.rows,
            [
                // A provider that was never asked about is only needed.
                row(
                    "java-common",
                    "Needed by what you're installing",
                    "2 kB",
                    false
                ),
                row("jre21-openjdk", "You picked this", "3.0 MB", true),
                row(
                    "nss",
                    "Needed by what you're installing",
                    "Already downloaded",
                    false
                ),
                row("dbeaver", "You asked for this", "100.0 MB", true),
            ]
        );
    }

    #[test]
    fn a_group_member_kept_in_the_question_was_picked() {
        let mut model = InstallModel::new();
        let ticket = model.open_group("plasma").unwrap();
        model.done(
            ticket.number,
            Ok(WorkAnswer::Resolved(Resolution::NeedsAnswers(vec![group(
                &[("kwin", None)],
            )]))),
        );
        let ticket = model.go_on().unwrap();
        let plan = plan_of(vec![item("kwin", Role::Target, 10, 20, None)]);
        model.done(
            ticket.number,
            Ok(WorkAnswer::Resolved(Resolution::Ready(plan))),
        );
        let view = install_view(&model);
        assert_eq!(view.title_before, "Review: install plasma");
        assert_eq!(view.rows, [row("kwin", "You picked this", "10 B", true)]);
    }

    #[test]
    fn back_from_the_review_returns_to_the_question_with_the_pick_kept() {
        let mut model = picked_a_provider();
        assert_eq!(model.back(), crate::install_model::Back::Stayed);
        let view = install_view(&model);
        assert_eq!(view.kind, CardKind::Provider);
        assert!(view.options[1].checked);
    }

    #[test]
    fn the_tiles_count_the_packages_the_download_and_the_extra_disk_space() {
        let view = install_view(&picked_a_provider());
        assert_eq!(
            view.tiles,
            [
                tile("Packages", "4", ""),
                tile("Download", "103.0", "MB"),
                tile("Extra disk space", "134.4", "MB"),
            ]
        );
    }

    #[test]
    fn with_everything_cached_there_is_nothing_to_download() {
        let view = reviewing(vec![item("dbeaver", Role::Target, 0, 2_000_000, None)]);
        assert_eq!(
            view.tiles[1],
            TileView {
                words: true,
                ..tile("Download", "Nothing to download", "")
            }
        );
        assert_eq!(view.rows[0].size, "Already downloaded");
    }

    #[test]
    fn extra_disk_space_is_the_new_sizes_less_what_is_installed_now() {
        // An upgrade of 2 MB over 1 MB, and a new package of 500 kB.
        let view = reviewing(vec![
            item("dbeaver", Role::Target, 10, 2_000_000, Some(1_000_000)),
            item("nss", Role::Dependency, 10, 500_000, None),
        ]);
        assert_eq!(view.tiles[2], tile("Extra disk space", "1.5", "MB"));
    }

    #[test]
    fn a_reinstall_takes_no_extra_disk_space() {
        let view = reviewing(vec![item(
            "dbeaver",
            Role::Target,
            0,
            2_000_000,
            Some(2_000_000),
        )]);
        assert_eq!(view.tiles[2], tile("Extra disk space", "0", "B"));
    }

    #[test]
    fn a_smaller_result_is_disk_space_freed_with_a_positive_number() {
        let view = reviewing(vec![item(
            "dbeaver",
            Role::Target,
            10,
            1_000_000,
            Some(3_100_000),
        )]);
        assert_eq!(view.tiles[2], tile("Disk space freed", "2.1", "MB"));
        assert_eq!(
            view.technical[1],
            "The Disk space freed total is added up from pacman's rounded figures."
        );
    }

    #[test]
    fn the_technical_details_keep_pacmans_words() {
        let view = reviewing(vec![
            item("nss", Role::Dependency, 0, 20, None),
            item("dbeaver", Role::Target, 3_337_523, 20, None),
        ]);
        assert_eq!(
            view.technical,
            [
                "extra/nss 1.0-1 · download 0 bytes · Installed Size : nss KiB",
                "extra/dbeaver 1.0-1 · download 3337523 bytes · Installed Size : dbeaver KiB",
                "The Extra disk space total is added up from pacman's rounded figures.",
                "pacman counts in MiB (1 MiB = 1,048,576 bytes)",
            ]
        );
    }

    #[test]
    fn a_plan_that_updates_the_system_never_reaches_the_review_yet() {
        let mut plan = plan_of(vec![item("zsh", Role::Update, 10, 20, Some(20))]);
        plan.decision = InstallDecision::MustUpdateSystem {
            count: 1,
            ignored: 0,
        };
        let view = install_view(&answered("dbeaver", Resolution::Ready(plan)));
        assert_eq!(view.kind, CardKind::Stale);
        assert!(view.rows.is_empty() && view.tiles.is_empty());
    }

    #[test]
    fn the_heading_names_a_group_or_counts_packages() {
        let mut model = InstallModel::new();
        model.open_group("plasma");
        assert_eq!(install_view(&model).heading, "Install plasma");
    }

    #[test]
    fn focus_goes_to_the_list_or_else_the_first_usable_button() {
        let list = |questions| install_view(&asking("dbeaver", questions)).focus;
        assert_eq!(
            list(vec![provider("a", &["a1", "a2"], 0)]),
            FocusTarget::List
        );
        assert_eq!(list(vec![installed("zsh", false)]), FocusTarget::List);
        assert_eq!(list(vec![group(&[("a", None)])]), FocusTarget::List);
        // nothing to pick in a group of installed members: the only button
        assert_eq!(
            list(vec![group(&[("a", Some("1-1"))])]),
            FocusTarget::Secondary
        );
        // a button that is off cannot take it
        let mut model = InstallModel::new();
        let ticket = model.open(pkg("dbeaver")).unwrap();
        model.done(ticket.number, Ok(WorkAnswer::MustUpdate));
        assert_eq!(install_view(&model).focus, FocusTarget::Secondary);
        assert_eq!(
            install_view(&asking("vlc", vec![installed("vlc", true)])).focus,
            FocusTarget::Secondary
        );
        assert_eq!(
            install_view(&asking(
                "dbeaver",
                vec![installed("vlc", true), provider("a", &["a1", "a2"], 0)]
            ))
            .focus,
            FocusTarget::Primary
        );
        let renamed = blocked(
            "cron",
            vec![Blocker::ReplacedBy {
                requested: pkg("cron"),
                by: vec![pkg("cronie")],
            }],
        );
        assert_eq!(install_view(&renamed).focus, FocusTarget::Primary);
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

    #[test]
    fn the_card_says_which_lists_were_used_when_they_are_not_fresh() {
        let shown = |why, age_days| {
            let mut model = InstallModel::new();
            let ticket = model.open(pkg("dbeaver")).expect("starts at once");
            model.lists_checked(ticket.number, Some(ListsNote { why, age_days }));
            model.done(
                ticket.number,
                Ok(WorkAnswer::Resolved(Resolution::Ready(plan(1)))),
            );
            install_view(&model).lists_note
        };
        assert_eq!(
            shown(NotFresh::Unavailable, Some(0)),
            "Checked with the package lists updated today."
        );
        assert_eq!(
            shown(NotFresh::Unavailable, Some(1)),
            "Checked with the package lists updated 1 day ago."
        );
        assert_eq!(
            shown(NotFresh::Unavailable, None),
            "Checked with the package lists already on this computer."
        );
        assert_eq!(
            shown(NotFresh::Busy, Some(12)),
            "Checked with the package lists updated 12 days ago. \
             Another refresh of the package lists is still running."
        );
        assert_eq!(
            shown(NotFresh::Failed, Some(3)),
            "Checked with the package lists updated 3 days ago. Couldn't get newer ones."
        );
    }

    #[test]
    fn with_fresh_lists_there_is_no_note() {
        let model = answered("dbeaver", Resolution::Ready(plan(1)));
        assert_eq!(install_view(&model).lists_note, "");
    }
}
