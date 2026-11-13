//! Turns the `SearchModel` into the exact words and flags the Search screen shows.
//! No Slint types here, so every string the window displays is unit-tested.

use std::time::SystemTime;

use crate::apps_view::RowUpdate;
use crate::error_words::ErrorView;
use crate::search_model::SearchModel;
use tidewise_pacman::Dependency;

/// The example searches on the start card, in order.
pub const EXAMPLES: [&str; 4] = ["java", "video", "vlc", "spotfy"];

/// What fills the area under the search box.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Body {
    /// Nothing typed: the start card.
    #[default]
    Start,
    /// pacman has no package lists: the explanation card.
    NoLists,
    /// Grey skeleton rows.
    Loading,
    /// The results list and the details.
    Results,
    /// The search found nothing.
    NoResults,
    /// The amber error panel.
    Error(ErrorView),
    /// The search text was refused: only the note under the search box (`inline_note`) shows.
    BadQuery,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StartView {
    pub title: String,
    pub body: String,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NoListsView {
    pub title: String,
    pub body: String,
    /// One line instead of a button: Tidewise can't get the lists yet.
    pub next_step: String,
    /// Behind the "Technical details" toggle: one line per repository without a list.
    pub technical: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NoResultsView {
    pub title: String,
    pub body: String,
    pub button: String,
}

/// One row of the results list, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRowView {
    /// The letter in the tile: first character of the name, upper-case.
    pub letter: String,
    pub name: String,
    pub version: String,
    pub repo: String,
    /// "", "testing" or "third-party".
    pub tag: String,
    pub installed: bool,
    pub description: String,
    pub selected: bool,
}

/// What the details panel is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailsKind {
    Loading,
    Found,
    NotFound,
    Failed,
}

/// The details panel for the selected package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsView {
    pub kind: DetailsKind,
    pub letter: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub repo: String,
    /// "", "testing" or "third-party".
    pub repo_tag: String,
    /// "Already installed ...", or empty.
    pub installed_note: String,
    /// "Matched because it provides <x>", or empty.
    pub matched_note: String,
    /// Shown instead of the numbers: "Loading the details…" or the not-found note. Empty when found.
    pub message: String,
    pub download_number: String,
    pub download_unit: String,
    pub installed_number: String,
    pub installed_unit: String,
    /// "3 packages", or empty when it needs nothing.
    pub needs_count: String,
    /// Up to five names.
    pub needs: Vec<String>,
    /// "and 2 more", or empty.
    pub needs_more: String,
    /// "Needs nothing else.", or empty.
    pub needs_nothing: String,
    /// Up to five things it also counts as, like `java-runtime=25`.
    pub provides: Vec<String>,
    /// "and 2 more", or empty.
    pub provides_more: String,
    /// "Looking only. Installing comes in Tidewise 0.1."
    pub note: String,
    /// The amber error words when `kind` is `Failed`.
    pub error: Option<ErrorView>,
    /// Behind the "Technical details" toggle, one line each.
    pub technical: Vec<String>,
}

/// Everything the Search screen shows right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchView {
    /// "Package lists updated 3 days ago" and its variants.
    pub subtitle: String,
    /// "This is taking longer than usual.", or empty.
    pub status: String,
    /// The refused-search words under the search box, or empty.
    pub inline_note: String,
    pub body: Body,
    pub start: StartView,
    pub no_lists: NoListsView,
    pub no_results: NoResultsView,
    /// The line above the rows.
    pub caption: String,
    pub rows: Vec<ResultRowView>,
    /// Position of the selected row in `rows` (so the window can scroll it into view).
    pub selected_index: Option<usize>,
    pub footer: String,
    pub details: Option<DetailsView>,
}

impl Default for DetailsView {
    fn default() -> Self {
        Self {
            kind: DetailsKind::Loading,
            letter: String::new(),
            name: String::new(),
            description: String::new(),
            version: String::new(),
            repo: String::new(),
            repo_tag: String::new(),
            installed_note: String::new(),
            matched_note: String::new(),
            message: String::new(),
            download_number: String::new(),
            download_unit: String::new(),
            installed_number: String::new(),
            installed_unit: String::new(),
            needs_count: String::new(),
            needs: Vec::new(),
            needs_more: String::new(),
            needs_nothing: String::new(),
            provides: Vec::new(),
            provides_more: String::new(),
            note: String::new(),
            error: None,
            technical: Vec::new(),
        }
    }
}

/// The example search at this position on the start card, or `None` if there is no such example.
pub fn example_query(_index: i32) -> Option<&'static str> {
    // Stub for the red commit.
    None
}

/// `glibc>=2.40`, `java-runtime=25` or just `bash`: a name with its version rule, the way pacman prints it.
pub fn dependency_text(_dependency: &Dependency) -> String {
    // Stub for the red commit.
    String::new()
}

/// Compares the rows on screen with the rows that should be on screen.
pub fn row_update(_old: &[ResultRowView], _new: &[ResultRowView]) -> RowUpdate {
    // Stub for the red commit.
    RowUpdate::Replace
}

/// Builds the whole screen from the model. `now` is the caller's clock (the model has none).
pub fn search_view(_model: &SearchModel, _now: SystemTime) -> SearchView {
    // Stub for the red commit.
    SearchView::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};
    use tidewise_pacman::{
        Comparison, DetailsLookup, PackageDetails, PacmanError, SearchResult, SyncLists,
        VersionRule,
    };

    const DAY: u64 = 86_400;
    /// 2025-10-14 00:00:00 UTC.
    const LISTS_AT: u64 = 1_760_400_000;

    fn at(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    /// An hour into the day that is `days` days after the lists were made.
    fn now(days: u64) -> SystemTime {
        at(LISTS_AT + days * DAY + 3_600)
    }

    fn lists(newest: Option<u64>, missing: &[&str]) -> Result<SyncLists, PacmanError> {
        Ok(SyncLists {
            newest: newest.map(at),
            missing: missing.iter().map(|repo| repo.to_string()).collect(),
        })
    }

    fn hit(repo: &str, name: &str, description: &str) -> SearchResult {
        SearchResult {
            repo: repo.to_string(),
            name: name.to_string(),
            version: "1.0-1".to_string(),
            groups: Vec::new(),
            installed_version: None,
            description: description.to_string(),
        }
    }

    fn extra(name: &str, description: &str) -> SearchResult {
        hit("extra", name, description)
    }

    fn failed() -> PacmanError {
        PacmanError::Failed {
            code: Some(1),
            stderr: "error: boom".to_string(),
        }
    }

    /// A model with package lists from `LISTS_AT`.
    fn with_lists() -> SearchModel {
        let mut model = SearchModel::new();
        model.set_sync_lists(lists(Some(LISTS_AT), &[]));
        model
    }

    /// A model that has answered `query` with `results`.
    fn answered(query: &str, results: Vec<SearchResult>) -> SearchModel {
        let mut model = with_lists();
        model.set_query(query);
        let ticket = model.start_search().unwrap();
        model.search_done(ticket.number, Ok(results));
        model
    }

    fn view(model: &SearchModel) -> SearchView {
        search_view(model, now(3))
    }

    // ---- header ----

    #[test]
    fn the_subtitle_says_how_old_the_package_lists_are() {
        let model = with_lists();
        let subtitle = |days| search_view(&model, now(days)).subtitle;
        assert_eq!(subtitle(0), "Package lists updated today");
        assert_eq!(subtitle(1), "Package lists updated 1 day ago");
        assert_eq!(subtitle(2), "Package lists updated 2 days ago");
        assert_eq!(subtitle(30), "Package lists updated 30 days ago");
    }

    #[test]
    fn a_clock_behind_the_lists_still_says_today() {
        let model = with_lists();
        let early = at(LISTS_AT - 5 * DAY);
        assert_eq!(
            search_view(&model, early).subtitle,
            "Package lists updated today"
        );
    }

    #[test]
    fn the_subtitle_for_lists_not_checked_missing_or_unreadable() {
        assert_eq!(
            view(&SearchModel::new()).subtitle,
            "Checking the package lists…"
        );
        let mut none = SearchModel::new();
        none.set_sync_lists(lists(None, &["core"]));
        assert_eq!(view(&none).subtitle, "No package lists yet");
        let mut broken = SearchModel::new();
        broken.set_sync_lists(Err(failed()));
        assert_eq!(
            view(&broken).subtitle,
            "Couldn't check how new the package lists are"
        );
    }

    #[test]
    fn a_slow_search_adds_the_longer_than_usual_note() {
        let mut model = with_lists();
        model.set_query("vlc");
        model.start_search();
        assert_eq!(view(&model).status, "");
        model.mark_slow();
        assert_eq!(view(&model).status, "This is taking longer than usual.");
    }

    // ---- the start card, the no-lists card, loading ----

    #[test]
    fn with_nothing_typed_the_start_card_shows() {
        let view = view(&with_lists());
        assert_eq!(view.body, Body::Start);
        assert_eq!(view.start.title, "Type to search your package lists");
        assert_eq!(
            view.start.body,
            "Search looks at package names and descriptions, like pacman -Ss. \
             AUR packages are not included."
        );
        assert_eq!(view.start.examples, ["java", "video", "vlc", "spotfy"]);
        assert!(view.rows.is_empty());
        assert_eq!(view.details, None);
        assert_eq!(view.footer, "");
        assert_eq!(view.inline_note, "");
    }

    #[test]
    fn the_examples_can_be_picked_by_position() {
        assert_eq!(example_query(0), Some("java"));
        assert_eq!(example_query(3), Some("spotfy"));
        assert_eq!(example_query(4), None);
        assert_eq!(example_query(-1), None);
    }

    #[test]
    fn without_package_lists_the_explanation_card_shows_with_no_button() {
        let mut model = SearchModel::new();
        model.set_sync_lists(lists(None, &["core", "extra"]));
        model.set_query("vlc"); // typed text does not matter: nothing can be searched
        let view = view(&model);
        assert_eq!(view.body, Body::NoLists);
        assert_eq!(
            view.no_lists.title,
            "Tidewise has no package lists to search yet"
        );
        assert_eq!(
            view.no_lists.body,
            "pacman keeps a list of every package in the repositories, but this system hasn't \
             downloaded it yet (usually right after installing Arch). Getting the package lists \
             from Tidewise comes in Tidewise 0.1."
        );
        assert_eq!(
            view.no_lists.next_step,
            "Until then, running sudo pacman -Syu in a terminal downloads them and updates your system."
        );
        assert_eq!(
            view.no_lists.technical,
            ["core: no package list", "extra: no package list"]
        );
    }

    #[test]
    fn a_running_first_search_shows_the_skeleton() {
        let mut model = with_lists();
        model.set_query("vlc");
        model.start_search();
        let view = view(&model);
        assert_eq!(view.body, Body::Loading);
        assert!(view.rows.is_empty());
        assert_eq!(view.footer, "");
        assert_eq!(view.details, None);
    }

    // ---- results ----

    #[test]
    fn each_result_row_has_its_texts_and_one_is_selected() {
        let mut vlc = extra("vlc", "Video player");
        vlc.installed_version = Some("1.0-1".to_string());
        let model = answered("vlc", vec![extra("vlc-plugin", "Plugin"), vlc]);
        let view = view(&model);
        assert_eq!(view.body, Body::Results);
        assert_eq!(view.rows.len(), 2);
        // Ranking put the exact name first.
        assert_eq!(
            view.rows[0],
            ResultRowView {
                letter: "V".to_string(),
                name: "vlc".to_string(),
                version: "1.0-1".to_string(),
                repo: "extra".to_string(),
                tag: String::new(),
                installed: true,
                description: "Video player".to_string(),
                selected: true,
            }
        );
        assert!(!view.rows[1].installed);
        assert!(!view.rows[1].selected);
        assert_eq!(view.selected_index, Some(0));
    }

    #[test]
    fn testing_and_third_party_repositories_get_a_tag() {
        let model = answered(
            "vlc",
            vec![
                hit("core", "vlc", ""),
                hit("extra-testing", "vlc2", ""),
                hit("chaotic-aur", "vlc3", ""),
            ],
        );
        let screen = view(&model);
        let tags: Vec<&str> = screen.rows.iter().map(|r| r.tag.as_str()).collect();
        assert_eq!(tags, ["", "testing", "third-party"]);
    }

    #[test]
    fn the_tile_letter_is_the_first_character_upper_cased() {
        let model = answered("é", vec![extra("épiphany", ""), extra("7zip é", "")]);
        let letters: Vec<String> = view(&model).rows.iter().map(|r| r.letter.clone()).collect();
        assert_eq!(letters, ["É", "7"]);
    }

    #[test]
    fn the_caption_says_which_groups_the_list_has() {
        let name = || extra("video-tool", "Does things");
        let description = || extra("kdenlive", "Video editor");
        let provides = || extra("flac", "Free Lossless Audio Codec");
        let caption = |results: Vec<SearchResult>| view(&answered("video", results)).caption;
        assert_eq!(caption(vec![name()]), "Package");
        assert_eq!(
            caption(vec![name(), description()]),
            "Name matches first, then description matches"
        );
        assert_eq!(
            caption(vec![name(), provides()]),
            "Name matches first, then packages that provide it"
        );
        assert_eq!(
            caption(vec![description(), provides()]),
            "Description matches first, then packages that provide it"
        );
        assert_eq!(
            caption(vec![name(), description(), provides()]),
            "Name matches first, then description matches, then packages that provide it"
        );
    }

    #[test]
    fn the_footer_counts_the_results() {
        let one = answered("vlc", vec![extra("vlc", "")]);
        assert_eq!(view(&one).footer, "1 result for “vlc”");
        let two = answered("vlc", vec![extra("vlc", ""), extra("vlc-x", "")]);
        assert_eq!(view(&two).footer, "2 results for “vlc”");
    }

    #[test]
    fn while_a_later_search_runs_the_rows_stay_and_the_footer_says_so() {
        let mut model = answered("vlc", vec![extra("vlc", "")]);
        model.set_query("  java ");
        model.start_search();
        let view = view(&model);
        assert_eq!(view.body, Body::Results);
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.footer, "Searching for “java”…");
    }

    #[test]
    fn no_results_says_so_with_the_searched_text_and_offers_to_clear() {
        let model = answered("spotfy", Vec::new());
        let view = view(&model);
        assert_eq!(view.body, Body::NoResults);
        assert_eq!(view.no_results.title, "No package matches “spotfy”");
        assert_eq!(
            view.no_results.body,
            "Check the spelling, or try a word from what the app does (“image editor” instead \
             of a brand name). Tidewise searches the repositories pacman knows about, not the AUR."
        );
        assert_eq!(view.no_results.button, "Clear search");
        assert!(view.rows.is_empty());
        assert_eq!(view.footer, "");
        assert_eq!(view.details, None);
        assert_eq!(view.selected_index, None);
    }

    // ---- errors ----

    #[test]
    fn a_pacman_failure_shows_the_amber_panel_for_a_search() {
        let mut model = with_lists();
        model.set_query("vlc");
        let ticket = model.start_search().unwrap();
        model.search_done(ticket.number, Err(failed()));
        let view = view(&model);
        let Body::Error(error) = view.body else {
            panic!("expected the Error body, got {:?}", view.body);
        };
        assert_eq!(error.title, "pacman couldn't finish the search");
        assert_eq!(error.technical, "exit code 1: error: boom");
        assert_eq!(error.unchanged, "Nothing on your system was changed.");
        assert_eq!(view.inline_note, "");
        assert!(view.rows.is_empty());
    }

    #[test]
    fn a_refused_search_text_shows_only_a_note_under_the_search_box() {
        let mut model = with_lists();
        model.set_query("a");
        let ticket = model.start_search().unwrap();
        model.search_done(
            ticket.number,
            Err(PacmanError::BadQuery {
                reason: "the search text is too long",
            }),
        );
        let view = view(&model);
        assert_eq!(view.body, Body::BadQuery);
        assert_eq!(
            view.inline_note,
            "That search can't be used. Tidewise didn't search because the text was too long or \
             had characters it can't use. Try a shorter, simpler search."
        );
        assert!(view.rows.is_empty());
    }

    // ---- details ----

    fn zsh() -> PackageDetails {
        let text = include_str!("../../../tests/fixtures/pacman/si-zsh.txt");
        tidewise_pacman::parse_si(text).unwrap().remove(0)
    }

    /// Details for `extra/<name>` with zsh's real sizes and (unless changed) its three dependencies.
    fn details_of(name: &str) -> PackageDetails {
        PackageDetails {
            repo: "extra".to_string(),
            name: name.to_string(),
            ..zsh()
        }
    }

    fn dependency(name: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
            rule: None,
        }
    }

    /// Answers the lookup of the selected package with `details`.
    fn give_details(model: &mut SearchModel, details: PackageDetails) {
        let package = model.request_details().expect("a lookup to start");
        let lookup = DetailsLookup {
            found: vec![details],
            not_found: Vec::new(),
        };
        model.details_done(&package, Ok(lookup));
    }

    #[test]
    fn there_are_no_details_without_a_selection() {
        assert_eq!(view(&with_lists()).details, None);
    }

    #[test]
    fn while_loading_the_details_the_known_facts_and_a_message_show() {
        let mut vlc = extra("vlc", "Video player");
        vlc.installed_version = Some("1.0-1".to_string());
        let model = answered("vlc", vec![vlc]);
        let details = view(&model).details.unwrap();
        assert_eq!(details.kind, DetailsKind::Loading);
        assert_eq!(details.letter, "V");
        assert_eq!(details.name, "vlc");
        assert_eq!(details.description, "Video player");
        assert_eq!(details.version, "1.0-1");
        assert_eq!(details.repo, "extra");
        assert_eq!(details.repo_tag, "");
        assert_eq!(details.installed_note, "Already installed");
        assert_eq!(details.message, "Loading the details…");
        assert_eq!(details.download_number, "");
        assert_eq!(
            details.note,
            "Looking only. Installing comes in Tidewise 0.1."
        );
        assert_eq!(
            details.technical,
            [
                "source: pacman -Si extra/vlc",
                "Package lists  : 2025-10-14 00:00:00 UTC"
            ]
        );
    }

    #[test]
    fn found_details_show_sizes_needs_and_pacmans_own_words() {
        let mut model = answered("vlc", vec![extra("vlc", "Video player")]);
        give_details(&mut model, details_of("vlc"));
        let details = view(&model).details.unwrap();
        assert_eq!(details.kind, DetailsKind::Found);
        assert_eq!(details.message, "");
        // zsh: 2.31 MiB to download, 6.94 MiB on disk.
        assert_eq!(
            (
                details.download_number.as_str(),
                details.download_unit.as_str()
            ),
            ("2.4", "MB")
        );
        assert_eq!(
            (
                details.installed_number.as_str(),
                details.installed_unit.as_str()
            ),
            ("7.3", "MB")
        );
        assert_eq!(details.needs_count, "3 packages");
        assert_eq!(details.needs, ["pcre2", "libcap", "gdbm"]);
        assert_eq!(details.needs_more, "");
        assert_eq!(details.needs_nothing, "");
        assert!(details.provides.is_empty());
        assert_eq!(details.provides_more, "");
        assert_eq!(
            details.technical,
            [
                "source: pacman -Si extra/vlc",
                "Repository     : extra",
                "Download Size  : 2.31 MiB",
                "Installed Size : 6.94 MiB",
                "Depends On     : pcre2  libcap  gdbm",
                "Package lists  : 2025-10-14 00:00:00 UTC",
            ]
        );
    }

    #[test]
    fn a_package_that_needs_nothing_says_so() {
        let mut model = answered("vlc", vec![extra("vlc", "")]);
        let lone = PackageDetails {
            depends_on: Vec::new(),
            ..details_of("vlc")
        };
        give_details(&mut model, lone);
        let details = view(&model).details.unwrap();
        assert_eq!(details.needs_count, "");
        assert!(details.needs.is_empty());
        assert_eq!(details.needs_nothing, "Needs nothing else.");
        assert!(
            details
                .technical
                .contains(&"Depends On     : None".to_string())
        );
    }

    #[test]
    fn one_need_is_singular_and_many_end_with_and_n_more() {
        let mut model = answered("vlc", vec![extra("vlc", "")]);
        let one = PackageDetails {
            depends_on: vec![dependency("glibc")],
            ..details_of("vlc")
        };
        give_details(&mut model, one);
        assert_eq!(view(&model).details.unwrap().needs_count, "1 package");

        let mut model = answered("vlc", vec![extra("vlc", "")]);
        let seven = PackageDetails {
            depends_on: ["a", "b", "c", "d", "e", "f", "g"].map(dependency).to_vec(),
            ..details_of("vlc")
        };
        give_details(&mut model, seven);
        let details = view(&model).details.unwrap();
        assert_eq!(details.needs_count, "7 packages");
        assert_eq!(details.needs, ["a", "b", "c", "d", "e"]);
        assert_eq!(details.needs_more, "and 2 more");
    }

    #[test]
    fn also_counts_as_lists_what_the_package_provides_with_versions() {
        let mut model = answered("jre", vec![extra("jre-openjdk", "Java")]);
        let provides =
            ["java-runtime=25", "jre=25", "a", "b", "c", "d", "e"].map(|text| {
                match text.split_once('=') {
                    Some((name, version)) => Dependency {
                        name: name.to_string(),
                        rule: Some(VersionRule {
                            comparison: Comparison::Equal,
                            version: version.to_string(),
                        }),
                    },
                    None => dependency(text),
                }
            });
        let java = PackageDetails {
            provides: provides.to_vec(),
            ..details_of("jre-openjdk")
        };
        give_details(&mut model, java);
        let details = view(&model).details.unwrap();
        assert_eq!(
            details.provides,
            ["java-runtime=25", "jre=25", "a", "b", "c"]
        );
        assert_eq!(details.provides_more, "and 2 more");
    }

    #[test]
    fn dependencies_are_written_the_way_pacman_prints_them() {
        let with = |comparison, version: &str| Dependency {
            name: "glibc".to_string(),
            rule: Some(VersionRule {
                comparison,
                version: version.to_string(),
            }),
        };
        assert_eq!(dependency_text(&dependency("bash")), "bash");
        assert_eq!(
            dependency_text(&with(Comparison::Equal, "2.40")),
            "glibc=2.40"
        );
        assert_eq!(dependency_text(&with(Comparison::Less, "3")), "glibc<3");
        assert_eq!(
            dependency_text(&with(Comparison::LessOrEqual, "3")),
            "glibc<=3"
        );
        assert_eq!(dependency_text(&with(Comparison::Greater, "1")), "glibc>1");
        assert_eq!(
            dependency_text(&with(Comparison::GreaterOrEqual, "1:2.0-1")),
            "glibc>=1:2.0-1"
        );
    }

    #[test]
    fn a_version_rule_shows_in_the_technical_depends_on_line() {
        let mut model = answered("vlc", vec![extra("vlc", "")]);
        let rule = Dependency {
            name: "glibc".to_string(),
            rule: Some(VersionRule {
                comparison: Comparison::GreaterOrEqual,
                version: "2.40".to_string(),
            }),
        };
        let versioned = PackageDetails {
            depends_on: vec![rule, dependency("bash")],
            ..details_of("vlc")
        };
        give_details(&mut model, versioned);
        let technical = view(&model).details.unwrap().technical;
        assert!(technical.contains(&"Depends On     : glibc>=2.40  bash".to_string()));
        // The chips show names only.
        assert_eq!(view(&model).details.unwrap().needs, ["glibc", "bash"]);
    }

    #[test]
    fn the_installed_note_says_when_the_installed_version_differs() {
        let mut old = extra("vlc", "");
        old.version = "3.0.21-12".to_string();
        old.installed_version = Some("3.0.20-1".to_string());
        let details = view(&answered("vlc", vec![old])).details.unwrap();
        assert_eq!(
            details.installed_note,
            "Already installed (version 3.0.20-1; the repository has 3.0.21-12)"
        );
        let not_installed = view(&answered("vlc", vec![extra("vlc", "")]))
            .details
            .unwrap();
        assert_eq!(not_installed.installed_note, "");
    }

    #[test]
    fn a_provides_only_result_says_why_it_matched() {
        // Real pacman data: dmd provides d-compiler=2.113.0 (not in its name or description).
        let text = include_str!("../../../tests/fixtures/pacman/si-dmd.txt");
        let dmd = tidewise_pacman::parse_si(text).unwrap().remove(0);
        let result = hit(
            &dmd.repo,
            &dmd.name,
            "Reference compiler for the D language",
        );
        let mut model = answered("d-compiler", vec![result]);
        // Before the details arrive it can only say it is about what the package provides.
        assert_eq!(
            view(&model).details.unwrap().matched_note,
            "Matched because of something it provides"
        );
        give_details(&mut model, dmd);
        assert_eq!(
            view(&model).details.unwrap().matched_note,
            "Matched because it provides d-compiler"
        );
    }

    #[test]
    fn a_name_or_description_match_has_no_because_note() {
        let model = answered("vlc", vec![extra("vlc", "Video player")]);
        assert_eq!(view(&model).details.unwrap().matched_note, "");
    }

    #[test]
    fn details_pacman_no_longer_knows_say_so() {
        let mut model = answered("vlc", vec![extra("vlc", "")]);
        let package = model.request_details().unwrap();
        let lookup = DetailsLookup {
            found: Vec::new(),
            not_found: vec![package.clone()],
        };
        model.details_done(&package, Ok(lookup));
        let details = view(&model).details.unwrap();
        assert_eq!(details.kind, DetailsKind::NotFound);
        assert_eq!(
            details.message,
            "This package isn't in your package lists any more"
        );
        assert_eq!(details.name, "vlc");
        assert_eq!(details.download_number, "");
    }

    #[test]
    fn a_failed_lookup_shows_the_amber_words_in_the_details_area() {
        let mut model = answered("vlc", vec![extra("vlc", "")]);
        let package = model.request_details().unwrap();
        model.details_done(&package, Err(failed()));
        let details = view(&model).details.unwrap();
        assert_eq!(details.kind, DetailsKind::Failed);
        assert_eq!(details.message, "");
        let error = details.error.unwrap();
        assert_eq!(error.title, "pacman couldn't look up this package");
        assert_eq!(error.technical, "exit code 1: error: boom");
    }

    #[test]
    fn testing_and_third_party_show_in_the_details_too() {
        let model = answered("vlc", vec![hit("extra-testing", "vlc", "")]);
        assert_eq!(view(&model).details.unwrap().repo_tag, "testing");
        let model = answered("vlc", vec![hit("chaotic-aur", "vlc", "")]);
        assert_eq!(view(&model).details.unwrap().repo_tag, "third-party");
    }

    // ---- updating the rows on screen ----

    fn row(repo: &str, name: &str, selected: bool) -> ResultRowView {
        ResultRowView {
            letter: "X".to_string(),
            name: name.to_string(),
            version: "1".to_string(),
            repo: repo.to_string(),
            tag: String::new(),
            installed: false,
            description: String::new(),
            selected,
        }
    }

    #[test]
    fn identical_rows_need_no_update() {
        let rows = [row("extra", "a", true), row("extra", "b", false)];
        assert_eq!(row_update(&rows, &rows), RowUpdate::Patch(vec![]));
    }

    #[test]
    fn moving_the_selection_patches_only_the_two_rows() {
        let old = [
            row("extra", "a", true),
            row("extra", "b", false),
            row("extra", "c", false),
        ];
        let new = [
            row("extra", "a", false),
            row("extra", "b", false),
            row("extra", "c", true),
        ];
        assert_eq!(row_update(&old, &new), RowUpdate::Patch(vec![0, 2]));
    }

    #[test]
    fn different_packages_length_or_order_replace_the_list() {
        let old = [row("extra", "a", true), row("extra", "b", false)];
        assert_eq!(
            row_update(&old, &[row("extra", "a", true)]),
            RowUpdate::Replace
        );
        assert_eq!(
            row_update(&old, &[row("extra", "b", true), row("extra", "a", false)]),
            RowUpdate::Replace
        );
        // The same name in another repository is another package.
        assert_eq!(
            row_update(
                &old,
                &[row("extra-testing", "a", true), row("extra", "b", false)]
            ),
            RowUpdate::Replace
        );
        assert_eq!(row_update(&[], &old), RowUpdate::Replace);
        assert_eq!(row_update(&old, &[]), RowUpdate::Replace);
    }
}
