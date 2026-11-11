//! Turns the `AppsModel` into the exact words and flags the Apps screen shows.
//! No Slint types here, so every string the window displays is unit-tested.

use crate::apps_model::{AppsModel, Details, EmptyState, ErrorText, ListState, Row, Tab};

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
    pub size: String,
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
    pub body: Body,
    pub rows: Vec<RowView>,
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

/// Builds the whole screen from the model. `typed_search` is the text exactly as typed (used in messages).
pub fn apps_view(model: &AppsModel, typed_search: &str) -> AppsView {
    AppsView {
        subtitle: subtitle(model),
        tab_counts: tab_counts(model),
        body: body(model, typed_search),
        rows: model.rows().iter().map(row_view).collect(),
        footer: footer(model),
        details: model.details().map(|d| details_view(&d)),
    }
}

fn subtitle(_model: &AppsModel) -> SubtitleView {
    SubtitleView::default() // stub
}

fn tab_counts(_model: &AppsModel) -> [String; 3] {
    Default::default() // stub
}

fn row_view(_row: &Row) -> RowView {
    RowView {
        letter: String::new(),
        name: String::new(),
        version: String::new(),
        description: String::new(),
        size: String::new(),
        selected: false,
    } // stub
}

fn footer(_model: &AppsModel) -> String {
    String::new() // stub
}

fn body(_model: &AppsModel, _typed_search: &str) -> Body {
    let _ = (ListState::Loading, EmptyState::NotEmpty); // stub
    Body::Loading
}

fn error_view(_text: &ErrorText) -> ErrorView {
    ErrorView {
        title: String::new(),
        body: String::new(),
        technical: String::new(),
        unchanged: String::new(),
        report: String::new(),
    } // stub
}

fn details_view(_details: &Details) -> DetailsView {
    DetailsView {
        name: String::new(),
        description: String::new(),
        size: String::new(),
        version: String::new(),
        why_here: String::new(),
        needed_by: Vec::new(),
        more: String::new(),
        nothing_needs_it: String::new(),
        note: String::new(),
        technical_source: String::new(),
        technical_bytes: String::new(),
    } // stub
}

/// Compares the rows on screen with the rows that should be on screen.
pub fn row_update(_old: &[RowView], _new: &[RowView]) -> RowUpdate {
    RowUpdate::Replace // stub
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
}
