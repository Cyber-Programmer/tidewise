//! Turns the `AppsModel` into the exact words and flags the Apps screen shows.
//! No Slint types here, so every string the window displays is unit-tested.

use crate::apps_model::{
    AppsModel, Details, EmptyState, ErrorText, ListState, Row, SortOrder, Tab,
};
use crate::size_text::split_size;

/// Shown while the list is loading, and as the "number" when there is none yet.
const NO_NUMBER: &str = "–";

/// The words above the list, and the numbers (shown in Plex Mono) when they are known.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubtitleView {
    /// Non-empty while loading or after an error: show this instead of the numbers.
    pub message: String,
    /// Non-empty when loading has taken more than 10 seconds.
    pub slow_note: String,
    /// All packages on the system.
    pub all: String,
    /// Packages the user installed.
    pub explicit: String,
}

/// One row of the list, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowView {
    /// The letter in the tile: first character of the name, upper-case.
    pub letter: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub size: String,
    pub selected: bool,
}

/// What a button in an empty state does when pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyAction {
    /// No button.
    None,
    ShowTab(Tab),
    ClearSearch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyView {
    pub title: String,
    pub body: String,
    /// Empty when there is no button.
    pub button: String,
    pub action: EmptyAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorView {
    pub title: String,
    pub body: String,
    /// Behind the "Technical details" toggle.
    pub technical: String,
    /// "Nothing on your system was changed."
    pub unchanged: String,
    /// Plain text, not a link in M1.
    pub report: String,
}

/// What fills the list panel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Body {
    /// Grey skeleton rows.
    #[default]
    Loading,
    List,
    Empty(EmptyView),
    Error(ErrorView),
}

/// The details panel for the selected package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsView {
    pub name: String,
    pub description: String,
    /// The whole size text, e.g. "1.67 GB".
    pub size: String,
    /// The number part of `size`, e.g. "1.67" (drawn big, in Plex Mono).
    pub size_number: String,
    /// The unit part of `size`, e.g. "GB".
    pub size_unit: String,
    pub version: String,
    pub why_here: String,
    /// Chips: up to five package names.
    pub needed_by: Vec<String>,
    /// "and 2 more", or empty.
    pub more: String,
    /// "Nothing else on this system needs it.", or empty when something does.
    pub nothing_needs_it: String,
    /// "Looking only. Removing packages comes in a later version of Tidewise."
    pub note: String,
    /// Behind the "Technical details" toggle: "source: pacman -Qi <name>".
    pub technical_source: String,
    /// Behind the toggle: "7 340 032 bytes".
    pub technical_bytes: String,
}

/// Everything the Apps screen shows right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppsView {
    pub subtitle: SubtitleView,
    /// Count texts for the tabs in order: Installed by you, Dependencies, All. "–" until loaded.
    pub tab_counts: [String; 3],
    /// Position of the current tab in the tab bar: 0 Installed by you, 1 Dependencies, 2 All.
    pub current_tab: i32,
    /// Position of the current sort in the sort bar: 0 Size, 1 Name.
    pub current_sort: i32,
    pub body: Body,
    pub rows: Vec<RowView>,
    /// Position of the selected row in `rows` (so the window can scroll it into view).
    pub selected_index: Option<usize>,
    /// "Showing N of M", or empty when there is no list.
    pub footer: String,
    pub details: Option<DetailsView>,
}

/// How to bring the list on screen up to date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowUpdate {
    /// Different rows: replace the whole list.
    Replace,
    /// Same rows in the same order: only these indexes changed (e.g. the selection moved).
    Patch(Vec<usize>),
}

/// The display name of a tab.
pub fn tab_name(tab: Tab) -> &'static str {
    match tab {
        Tab::Explicit => "Installed by you",
        Tab::Dependencies => "Dependencies",
        Tab::All => "All",
    }
}

/// The tab at this position in the tab bar, or `None` if there is no such tab.
pub fn tab_from_index(index: i32) -> Option<Tab> {
    match index {
        0 => Some(Tab::Explicit),
        1 => Some(Tab::Dependencies),
        2 => Some(Tab::All),
        _ => None,
    }
}

/// The sort at this position in the sort bar, or `None` if there is no such sort.
pub fn sort_from_index(index: i32) -> Option<SortOrder> {
    match index {
        0 => Some(SortOrder::SizeLargestFirst),
        1 => Some(SortOrder::NameAToZ),
        _ => None,
    }
}

/// Builds the whole screen from the model. `typed_search` is the text exactly as typed (used in messages).
pub fn apps_view(model: &AppsModel, typed_search: &str) -> AppsView {
    let rows: Vec<RowView> = model.rows().iter().map(row_view).collect();
    AppsView {
        selected_index: rows.iter().position(|row| row.selected),
        rows,
        subtitle: subtitle(model),
        tab_counts: tab_counts(model),
        current_tab: match model.tab() {
            Tab::Explicit => 0,
            Tab::Dependencies => 1,
            Tab::All => 2,
        },
        current_sort: match model.sort() {
            SortOrder::SizeLargestFirst => 0,
            SortOrder::NameAToZ => 1,
        },
        body: body(model, typed_search),
        footer: footer(model),
        details: model.details().map(|d| details_view(&d)),
    }
}

fn subtitle(model: &AppsModel) -> SubtitleView {
    match model.state() {
        ListState::Loaded => {
            let totals = model.totals();
            SubtitleView {
                all: totals.all.to_string(),
                explicit: totals.explicit.to_string(),
                ..SubtitleView::default()
            }
        }
        ListState::Loading => SubtitleView {
            message: "Asking pacman for the list of installed packages…".to_string(),
            slow_note: if model.is_slow() {
                "This is taking longer than usual.".to_string()
            } else {
                String::new()
            },
            all: NO_NUMBER.to_string(),
            explicit: NO_NUMBER.to_string(),
        },
        ListState::Error(_) => SubtitleView {
            message: "Couldn't read the list of installed packages.".to_string(),
            slow_note: String::new(),
            all: NO_NUMBER.to_string(),
            explicit: NO_NUMBER.to_string(),
        },
    }
}

fn tab_counts(model: &AppsModel) -> [String; 3] {
    if *model.state() != ListState::Loaded {
        return [NO_NUMBER, NO_NUMBER, NO_NUMBER].map(str::to_string);
    }
    let counts = model.tab_counts();
    [counts.explicit, counts.dependencies, counts.all].map(|n| n.to_string())
}

fn row_view(row: &Row) -> RowView {
    RowView {
        letter: row
            .name
            .chars()
            .next()
            .map_or("?".to_string(), |first| first.to_uppercase().collect()),
        name: row.name.clone(),
        version: row.version.clone(),
        description: row.description.clone(),
        size: row.size_text.clone(),
        selected: row.selected,
    }
}

fn footer(model: &AppsModel) -> String {
    if *model.state() != ListState::Loaded {
        return String::new();
    }
    let totals = model.totals();
    let of_this_tab = match model.tab() {
        Tab::Explicit => totals.explicit,
        Tab::Dependencies => totals.dependencies,
        Tab::All => totals.all,
    };
    format!("Showing {} of {}", model.rows().len(), of_this_tab)
}

fn body(model: &AppsModel, typed_search: &str) -> Body {
    match model.state() {
        ListState::Loading => Body::Loading,
        ListState::Error(text) => Body::Error(error_view(text)),
        ListState::Loaded => match model.empty_state() {
            EmptyState::NotEmpty => Body::List,
            EmptyState::OtherTabHas { tab, count } => {
                Body::Empty(other_tab_view(model.tab(), tab, count, typed_search.trim()))
            }
            EmptyState::NothingMatches => Body::Empty(nothing_matches_view(typed_search.trim())),
        },
    }
}

/// This tab is empty, but `other` has `count` packages that fit.
fn other_tab_view(this: Tab, other: Tab, count: usize, search: &str) -> EmptyView {
    let title = if search.is_empty() {
        format!("Nothing in {}", tab_name(this))
    } else {
        format!("No match in {} for “{search}”", tab_name(this))
    };
    let noun = match (search.is_empty(), count == 1) {
        (true, true) => "package",
        (true, false) => "packages",
        (false, true) => "match",
        (false, false) => "matches",
    };
    EmptyView {
        title,
        body: format!(
            "It is installed, just not in this tab: {count} {noun} in {}.",
            tab_name(other)
        ),
        button: format!("Show {}", tab_name(other)),
        action: EmptyAction::ShowTab(other),
    }
}

/// No tab has a match (or, with no search, there are no packages at all).
fn nothing_matches_view(search: &str) -> EmptyView {
    if search.is_empty() {
        return EmptyView {
            title: "No installed packages found".to_string(),
            body: String::new(),
            button: String::new(),
            action: EmptyAction::None,
        };
    }
    EmptyView {
        title: format!("No installed package matches “{search}”"),
        body: String::new(),
        button: "Clear search".to_string(),
        action: EmptyAction::ClearSearch,
    }
}

fn error_view(text: &ErrorText) -> ErrorView {
    ErrorView {
        title: text.title.clone(),
        body: text.body.clone(),
        technical: text.technical.clone(),
        unchanged: "Nothing on your system was changed.".to_string(),
        report: "Report problems at github.com/Cyber-Programmer/tidewise/issues".to_string(),
    }
}

fn details_view(details: &Details) -> DetailsView {
    let nobody = details.needed_by.is_empty();
    let (size_number, size_unit) = split_size(&details.size_text);
    DetailsView {
        name: details.name.clone(),
        description: details.description.clone(),
        size: details.size_text.clone(),
        size_number,
        size_unit,
        version: details.version.clone(),
        why_here: details.why_here.clone(),
        needed_by: details.needed_by.clone(),
        more: if details.needed_by_more > 0 {
            format!("and {} more", details.needed_by_more)
        } else {
            String::new()
        },
        nothing_needs_it: if nobody {
            "Nothing else on this system needs it.".to_string()
        } else {
            String::new()
        },
        note: "Looking only. Removing packages comes in a later version of Tidewise.".to_string(),
        technical_source: format!("source: pacman -Qi {}", details.name),
        technical_bytes: exact_bytes(details.size_bytes),
    }
}

/// "7 340 032 bytes": digits grouped in thousands with spaces.
fn exact_bytes(bytes: u64) -> String {
    if bytes == 1 {
        return "1 byte".to_string();
    }
    let digits = bytes.to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(' ');
        }
        grouped.push(digit);
    }
    format!("{grouped} bytes")
}

/// Compares the rows on screen with the rows that should be on screen.
pub fn row_update(old: &[RowView], new: &[RowView]) -> RowUpdate {
    let same_rows = old.len() == new.len()
        && old
            .iter()
            .zip(new)
            .all(|(before, after)| before.name == after.name);
    if !same_rows {
        return RowUpdate::Replace;
    }
    let changed = old
        .iter()
        .zip(new)
        .enumerate()
        .filter(|(_, (before, after))| before != after)
        .map(|(index, _)| index)
        .collect();
    RowUpdate::Patch(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_pacman::{InstallReason, InstalledPackage, PacmanError};

    fn pkg(
        name: &str,
        description: &str,
        size: u64,
        reason: InstallReason,
        required_by: &[&str],
    ) -> InstalledPackage {
        InstalledPackage {
            name: name.to_string(),
            version: "1.0-1".to_string(),
            description: description.to_string(),
            installed_size: size,
            reason,
            required_by: required_by.iter().map(|n| n.to_string()).collect(),
        }
    }

    /// 3 explicit (firefox, git, vim) and 2 dependencies (glibc, zlib).
    fn sample() -> Vec<InstalledPackage> {
        use InstallReason::{Dependency, Explicit};
        vec![
            pkg("firefox", "Web browser", 250_000_000, Explicit, &[]),
            pkg("git", "Version control", 40_000_000, Explicit, &[]),
            pkg(
                "glibc",
                "The C library",
                50_000_000,
                Dependency,
                &["firefox", "git"],
            ),
            pkg("vim", "Text editor", 40_000_000, Explicit, &[]),
            pkg("zlib", "Compression library", 300_000, Dependency, &["git"]),
        ]
    }

    fn loaded() -> AppsModel {
        let mut model = AppsModel::new();
        model.set_result(Ok(sample()));
        model
    }

    fn failed() -> AppsModel {
        let mut model = AppsModel::new();
        model.set_result(Err(PacmanError::Failed {
            code: Some(1),
            stderr: "error: unable to lock database".to_string(),
        }));
        model
    }

    #[test]
    fn the_subtitle_has_the_two_totals_once_loaded() {
        let view = apps_view(&loaded(), "");
        assert_eq!(view.subtitle.all, "5");
        assert_eq!(view.subtitle.explicit, "3");
        assert_eq!(view.subtitle.message, "");
        assert_eq!(view.subtitle.slow_note, "");
    }

    #[test]
    fn the_subtitle_totals_do_not_shrink_while_searching() {
        let mut model = loaded();
        model.set_search("library");
        let view = apps_view(&model, "library");
        assert_eq!(view.subtitle.all, "5");
        assert_eq!(view.subtitle.explicit, "3");
    }

    #[test]
    fn while_loading_the_subtitle_asks_pacman_and_shows_dashes() {
        let view = apps_view(&AppsModel::new(), "");
        assert_eq!(
            view.subtitle.message,
            "Asking pacman for the list of installed packages…"
        );
        assert_eq!(view.subtitle.slow_note, "");
        assert_eq!(view.subtitle.all, "–");
        assert_eq!(view.subtitle.explicit, "–");
        assert_eq!(view.tab_counts, ["–", "–", "–"]);
        assert_eq!(view.body, Body::Loading);
        assert!(view.rows.is_empty());
        assert_eq!(view.footer, "");
        assert_eq!(view.details, None);
    }

    #[test]
    fn a_slow_load_adds_the_longer_than_usual_note() {
        let mut model = AppsModel::new();
        assert!(model.begin_reload());
        model.mark_slow();
        let view = apps_view(&model, "");
        assert_eq!(view.subtitle.slow_note, "This is taking longer than usual.");
        assert_eq!(
            view.subtitle.message,
            "Asking pacman for the list of installed packages…"
        );
    }

    #[test]
    fn after_an_error_the_numbers_are_dashes_and_the_message_says_so() {
        let view = apps_view(&failed(), "");
        assert_eq!(
            view.subtitle.message,
            "Couldn't read the list of installed packages."
        );
        assert_eq!(view.subtitle.all, "–");
        assert_eq!(view.tab_counts, ["–", "–", "–"]);
        assert!(view.rows.is_empty());
        assert_eq!(view.footer, "");
    }

    #[test]
    fn the_tab_counts_follow_the_search() {
        let mut model = loaded();
        assert_eq!(apps_view(&model, "").tab_counts, ["3", "2", "5"]);
        model.set_search("library");
        assert_eq!(apps_view(&model, "library").tab_counts, ["0", "2", "2"]);
    }

    #[test]
    fn tabs_have_their_display_names() {
        assert_eq!(tab_name(Tab::Explicit), "Installed by you");
        assert_eq!(tab_name(Tab::Dependencies), "Dependencies");
        assert_eq!(tab_name(Tab::All), "All");
    }

    #[test]
    fn rows_carry_a_letter_and_the_texts() {
        let view = apps_view(&loaded(), "");
        assert_eq!(view.body, Body::List);
        let first = &view.rows[0];
        assert_eq!(first.letter, "F");
        assert_eq!(first.name, "firefox");
        assert_eq!(first.version, "1.0-1");
        assert_eq!(first.description, "Web browser");
        assert_eq!(first.size, "250.0 MB");
        assert!(first.selected);
        assert!(!view.rows[1].selected);
    }

    #[test]
    fn the_tile_letter_is_the_first_character_upper_cased() {
        let row = |name: &str| Row {
            name: name.to_string(),
            version: String::new(),
            description: String::new(),
            size_text: String::new(),
            selected: false,
        };
        assert_eq!(row_view(&row("qt6-base")).letter, "Q");
        assert_eq!(row_view(&row("7zip")).letter, "7");
        assert_eq!(row_view(&row("épiphany")).letter, "É");
        assert_eq!(row_view(&row("")).letter, "?");
    }

    #[test]
    fn the_footer_says_how_many_of_the_tab_are_shown() {
        let mut model = loaded();
        assert_eq!(apps_view(&model, "").footer, "Showing 3 of 3");
        model.set_search("vim");
        assert_eq!(apps_view(&model, "vim").footer, "Showing 1 of 3");
        model.set_tab(Tab::All);
        assert_eq!(apps_view(&model, "vim").footer, "Showing 1 of 5");
    }

    #[test]
    fn an_empty_tab_with_matches_elsewhere_points_to_that_tab() {
        let mut model = loaded();
        model.set_search("library"); // only dependencies match; the tab is "Installed by you"
        let view = apps_view(&model, "library");
        assert_eq!(
            view.body,
            Body::Empty(EmptyView {
                title: "No match in Installed by you for “library”".to_string(),
                body: "It is installed, just not in this tab: 2 matches in Dependencies."
                    .to_string(),
                button: "Show Dependencies".to_string(),
                action: EmptyAction::ShowTab(Tab::Dependencies),
            })
        );
        assert!(view.rows.is_empty());
    }

    #[test]
    fn one_match_elsewhere_is_singular_and_the_other_direction_works() {
        let mut model = loaded();
        model.set_tab(Tab::Dependencies);
        model.set_search("firefox");
        let view = apps_view(&model, "firefox");
        assert_eq!(
            view.body,
            Body::Empty(EmptyView {
                title: "No match in Dependencies for “firefox”".to_string(),
                body: "It is installed, just not in this tab: 1 match in Installed by you."
                    .to_string(),
                button: "Show Installed by you".to_string(),
                action: EmptyAction::ShowTab(Tab::Explicit),
            })
        );
    }

    #[test]
    fn the_message_quotes_the_search_trimmed_with_its_own_case() {
        let mut model = loaded();
        model.set_search("  Library ");
        let view = apps_view(&model, "  Library ");
        let Body::Empty(empty) = view.body else {
            panic!("expected an empty state");
        };
        assert_eq!(empty.title, "No match in Installed by you for “Library”");
    }

    #[test]
    fn when_nothing_matches_anywhere_the_screen_offers_clear_search() {
        let mut model = loaded();
        model.set_search("no such app");
        let view = apps_view(&model, "no such app");
        assert_eq!(
            view.body,
            Body::Empty(EmptyView {
                title: "No installed package matches “no such app”".to_string(),
                body: String::new(),
                button: "Clear search".to_string(),
                action: EmptyAction::ClearSearch,
            })
        );
        assert_eq!(view.footer, "Showing 0 of 3");
    }

    #[test]
    fn an_empty_tab_without_a_search_does_not_quote_an_empty_search() {
        use InstallReason::Dependency;
        let mut model = AppsModel::new();
        model.set_result(Ok(vec![pkg("glibc", "The C library", 1, Dependency, &[])]));
        let view = apps_view(&model, "");
        assert_eq!(
            view.body,
            Body::Empty(EmptyView {
                title: "Nothing in Installed by you".to_string(),
                body: "It is installed, just not in this tab: 1 package in Dependencies."
                    .to_string(),
                button: "Show Dependencies".to_string(),
                action: EmptyAction::ShowTab(Tab::Dependencies),
            })
        );
    }

    #[test]
    fn a_system_with_no_packages_at_all_has_no_button() {
        let mut model = AppsModel::new();
        model.set_result(Ok(Vec::new()));
        let view = apps_view(&model, "");
        assert_eq!(
            view.body,
            Body::Empty(EmptyView {
                title: "No installed packages found".to_string(),
                body: String::new(),
                button: String::new(),
                action: EmptyAction::None,
            })
        );
    }

    #[test]
    fn an_error_shows_the_error_text_and_the_reassurance() {
        let view = apps_view(&failed(), "");
        assert_eq!(
            view.body,
            Body::Error(ErrorView {
                title: "pacman couldn't list your packages".to_string(),
                body: "pacman stopped with an error before it finished. \
                       This is usually temporary; trying again often works."
                    .to_string(),
                technical: "exit code 1: error: unable to lock database".to_string(),
                unchanged: "Nothing on your system was changed.".to_string(),
                report: "Report problems at github.com/Cyber-Programmer/tidewise/issues"
                    .to_string(),
            })
        );
    }

    #[test]
    fn details_show_the_texts_and_the_looking_only_note() {
        let mut model = loaded();
        model.set_tab(Tab::All);
        model.select("glibc");
        let details = apps_view(&model, "").details.unwrap();
        assert_eq!(details.name, "glibc");
        assert_eq!(details.description, "The C library");
        assert_eq!(details.size, "50.0 MB");
        assert_eq!(details.version, "1.0-1");
        assert_eq!(details.why_here, "Another package needs it");
        assert_eq!(details.needed_by, ["firefox", "git"]);
        assert_eq!(details.more, "");
        assert_eq!(details.nothing_needs_it, "");
        assert_eq!(
            details.note,
            "Looking only. Removing packages comes in a later version of Tidewise."
        );
        assert_eq!(details.technical_source, "source: pacman -Qi glibc");
        assert_eq!(details.technical_bytes, "50 000 000 bytes");
    }

    #[test]
    fn a_package_nothing_needs_says_so() {
        let details = apps_view(&loaded(), "").details.unwrap();
        assert_eq!(details.name, "firefox");
        assert!(details.needed_by.is_empty());
        assert_eq!(
            details.nothing_needs_it,
            "Nothing else on this system needs it."
        );
        assert_eq!(details.more, "");
    }

    #[test]
    fn more_than_five_needers_end_with_and_n_more() {
        use InstallReason::Explicit;
        let seven = ["a", "b", "c", "d", "e", "f", "g"];
        let mut model = AppsModel::new();
        model.set_result(Ok(vec![pkg("seven", "", 1, Explicit, &seven)]));
        let details = apps_view(&model, "").details.unwrap();
        assert_eq!(details.needed_by.len(), 5);
        assert_eq!(details.more, "and 2 more");
        assert_eq!(details.nothing_needs_it, "");
        assert_eq!(details.technical_bytes, "1 byte");
    }

    #[test]
    fn exact_bytes_are_grouped_in_thousands() {
        use InstallReason::Explicit;
        let bytes = |n: u64| {
            let mut model = AppsModel::new();
            model.set_result(Ok(vec![pkg("p", "", n, Explicit, &[])]));
            apps_view(&model, "").details.unwrap().technical_bytes
        };
        assert_eq!(bytes(0), "0 bytes");
        assert_eq!(bytes(999), "999 bytes");
        assert_eq!(bytes(1_000), "1 000 bytes");
        assert_eq!(bytes(7_340_032), "7 340 032 bytes");
        assert_eq!(bytes(u64::MAX), "18 446 744 073 709 551 615 bytes");
    }

    fn row(name: &str, selected: bool) -> RowView {
        RowView {
            letter: "X".to_string(),
            name: name.to_string(),
            version: "1".to_string(),
            description: String::new(),
            size: "1 B".to_string(),
            selected,
        }
    }

    #[test]
    fn identical_rows_need_no_update() {
        let rows = [row("a", true), row("b", false)];
        assert_eq!(row_update(&rows, &rows), RowUpdate::Patch(vec![]));
    }

    #[test]
    fn moving_the_selection_patches_only_the_two_rows() {
        let old = [row("a", true), row("b", false), row("c", false)];
        let new = [row("a", false), row("b", false), row("c", true)];
        assert_eq!(row_update(&old, &new), RowUpdate::Patch(vec![0, 2]));
    }

    #[test]
    fn different_names_length_or_order_replace_the_list() {
        let old = [row("a", true), row("b", false)];
        assert_eq!(row_update(&old, &[row("a", true)]), RowUpdate::Replace);
        assert_eq!(
            row_update(&old, &[row("b", true), row("a", false)]),
            RowUpdate::Replace
        );
        assert_eq!(row_update(&[], &old), RowUpdate::Replace);
        assert_eq!(row_update(&old, &[]), RowUpdate::Replace);
    }

    #[test]
    fn the_bars_show_the_current_tab_and_sort() {
        let mut model = loaded();
        let view = apps_view(&model, "");
        assert_eq!((view.current_tab, view.current_sort), (0, 0));
        model.set_tab(Tab::All);
        model.set_sort(SortOrder::NameAToZ);
        let view = apps_view(&model, "");
        assert_eq!((view.current_tab, view.current_sort), (2, 1));
        model.set_tab(Tab::Dependencies);
        assert_eq!(apps_view(&model, "").current_tab, 1);
    }

    #[test]
    fn positions_in_the_bars_map_back_to_tabs_and_sorts() {
        assert_eq!(tab_from_index(0), Some(Tab::Explicit));
        assert_eq!(tab_from_index(1), Some(Tab::Dependencies));
        assert_eq!(tab_from_index(2), Some(Tab::All));
        assert_eq!(tab_from_index(3), None);
        assert_eq!(tab_from_index(-1), None);
        assert_eq!(sort_from_index(0), Some(SortOrder::SizeLargestFirst));
        assert_eq!(sort_from_index(1), Some(SortOrder::NameAToZ));
        assert_eq!(sort_from_index(2), None);
        assert_eq!(sort_from_index(-1), None);
    }

    #[test]
    fn the_view_says_where_the_selected_row_is() {
        let mut model = loaded();
        model.set_tab(Tab::All); // firefox, glibc, git, vim, zlib
        assert_eq!(apps_view(&model, "").selected_index, Some(0));
        model.select("vim");
        assert_eq!(apps_view(&model, "").selected_index, Some(3));
        model.select_last();
        assert_eq!(apps_view(&model, "").selected_index, Some(4));
    }

    #[test]
    fn the_view_has_no_selected_index_without_rows() {
        assert_eq!(apps_view(&AppsModel::new(), "").selected_index, None);
        assert_eq!(apps_view(&failed(), "").selected_index, None);
        let mut model = loaded();
        model.set_search("no such app");
        assert_eq!(apps_view(&model, "no such app").selected_index, None);
    }

    #[test]
    fn the_big_size_is_split_into_number_and_unit() {
        let split = |bytes: u64| {
            use InstallReason::Explicit;
            let mut model = AppsModel::new();
            model.set_result(Ok(vec![pkg("p", "", bytes, Explicit, &[])]));
            let details = apps_view(&model, "").details.unwrap();
            (details.size_number, details.size_unit, details.size)
        };
        assert_eq!(split(0), ("0".into(), "B".into(), "0 B".into()));
        assert_eq!(split(999_499), ("999".into(), "kB".into(), "999 kB".into()));
        assert_eq!(
            split(7_340_032),
            ("7.3".into(), "MB".into(), "7.3 MB".into())
        );
        assert_eq!(
            split(1_670_000_000),
            ("1.67".into(), "GB".into(), "1.67 GB".into())
        );
    }
}
