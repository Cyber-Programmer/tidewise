//! View model for the Search screen: the query, the ranked results, the selection and the details.
//! Plain Rust, no Slint, no threads, no clock. It never runs pacman; it is handed pacman's answers.

use crate::error_words::{Doing, ErrorText, error_text};
use crate::list_scroll::moved_selection;
use tidewise_pacman::{PackageRef, PacmanError, SearchResult};

/// Why a result is in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    /// Every search word is in the package name.
    Name,
    /// Every search word is in the name or the description.
    Description,
    /// pacman matched something the package *provides* (for example `c++` finds flac through `libFLAC++.so`).
    Provides,
}

/// Which kind of repository a package comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoKind {
    /// `core`, `extra` or `multilib`: shown plain.
    Official,
    /// A repository whose name ends in `-testing`: shown with a "testing" tag.
    Testing,
    /// Anything else: shown with a "third-party" tag.
    ThirdParty,
}

/// A search result with the reasons the screen needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedResult {
    pub result: SearchResult,
    pub matched: MatchKind,
    pub repo_kind: RepoKind,
}

/// Is this result the package `package` names? (A name alone is not enough: repositories can repeat names.)
fn is_package(result: &RankedResult, package: &PackageRef) -> bool {
    result.result.repo == package.repo && result.result.name == package.name
}

/// The repositories Arch itself ships. The same list lives in `tidewise_pacman::SearchResult::is_official`
/// (kept private there); this one works from a bare repository name.
const OFFICIAL_REPOS: [&str; 3] = ["core", "extra", "multilib"];

/// Which kind of repository this is, from its name.
pub fn repo_kind(repo: &str) -> RepoKind {
    if OFFICIAL_REPOS.contains(&repo) {
        RepoKind::Official
    } else if repo.ends_with("-testing") {
        RepoKind::Testing
    } else {
        RepoKind::ThirdParty
    }
}

/// Puts pacman's results in the order people expect: an exact name first, then name matches, then
/// description matches, then provides-only matches. Inside each group pacman's own order is kept.
///
/// pacman matches every search word against the name and description (and what the package provides),
/// ignoring case. So a result that is in neither text must have matched through what it provides.
pub fn rank(results: Vec<SearchResult>, query: &str) -> Vec<RankedResult> {
    let query = query.trim().to_lowercase();
    let terms: Vec<&str> = query.split_whitespace().collect();
    let mut ranked: Vec<(u8, RankedResult)> = results
        .into_iter()
        .map(|result| {
            let name = result.name.to_lowercase();
            let description = result.description.to_lowercase();
            let matched = if terms.iter().all(|term| name.contains(term)) {
                MatchKind::Name
            } else if terms
                .iter()
                .all(|term| name.contains(term) || description.contains(term))
            {
                MatchKind::Description
            } else {
                MatchKind::Provides
            };
            let group = match matched {
                MatchKind::Name if name == query => 0,
                MatchKind::Name => 1,
                MatchKind::Description => 2,
                MatchKind::Provides => 3,
            };
            let repo_kind = repo_kind(&result.repo);
            (
                group,
                RankedResult {
                    result,
                    matched,
                    repo_kind,
                },
            )
        })
        .collect();
    // A stable sort: results in the same group stay in pacman's order.
    ranked.sort_by_key(|(group, _)| *group);
    ranked.into_iter().map(|(_, ranked)| ranked).collect()
}

/// One search to run: the number says which question the answer belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchTicket {
    pub number: u64,
    /// The text to search for, already trimmed.
    pub query: String,
}

/// What the results area is doing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SearchState {
    /// Nothing typed: the start card.
    #[default]
    Start,
    /// A first search is running and there is nothing to show yet (skeleton rows).
    Loading,
    /// There is an answer (it may have no results; then the screen says so).
    Loaded,
    /// pacman failed.
    Error(ErrorText),
    /// The search text was refused before pacman started.
    BadQuery(ErrorText),
}

/// The Search screen's state. Set the inputs with the methods, read what to show with the others.
///
/// Searching is numbered and one at a time: only one `pacman -Ss` runs at once, a newer query waits its turn
/// (only the newest one is kept), and an answer that is not for the latest query is thrown away.
#[derive(Debug, Default)]
pub struct SearchModel {
    state: SearchState,
    results: Vec<RankedResult>,
    /// The text in the search box, trimmed.
    query: String,
    /// The query the current results (or error) answer.
    answered_query: String,
    /// The number given to the last search that was asked for.
    last_number: u64,
    /// The number of the search whose answer we want to show. `None` when nothing is wanted (nothing typed, or
    /// the answer has arrived): any other answer is stale.
    wanted: Option<u64>,
    /// The search the worker thread is running now.
    running: Option<SearchTicket>,
    /// A newer search that waits for the running one to finish. Only the newest is kept.
    pending: Option<SearchTicket>,
    slow: bool,
    /// The selected package. Always one of `results`, or `None` when there are none.
    selected: Option<PackageRef>,
}

impl SearchModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Remembers the text in the search box (trimmed). It does not search: `start_search` (called after the
    /// debounce) does. A blank text goes back to the start card at once and makes every answer still on its way
    /// stale.
    pub fn set_query(&mut self, text: &str) {
        self.query = text.trim().to_string();
        if self.query.is_empty() {
            self.wanted = None;
            self.pending = None;
            self.slow = false;
            self.state = SearchState::Start;
            self.results.clear();
            self.answered_query.clear();
            self.fix_selection();
        }
    }

    /// The text in the search box, trimmed.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Asks to search for the current text. `Some(ticket)`: start the worker now. `None`: nothing to search
    /// for, or a search is already running (this query waits and runs when that one answers).
    pub fn start_search(&mut self) -> Option<SearchTicket> {
        if self.query.is_empty() {
            return None;
        }
        // The running search is for exactly this text (the user typed something else and then came back):
        // its answer is the one we want, and anything that was waiting is out of date.
        if let Some(running) = &self.running
            && running.query == self.query
        {
            self.wanted = Some(running.number);
            self.pending = None;
            self.show_loading_unless_rows_are_shown();
            return None;
        }
        self.last_number += 1;
        let ticket = SearchTicket {
            number: self.last_number,
            query: self.query.clone(),
        };
        self.wanted = Some(ticket.number);
        self.slow = false;
        self.show_loading_unless_rows_are_shown();
        if self.running.is_some() {
            self.pending = Some(ticket);
            None
        } else {
            self.running = Some(ticket.clone());
            Some(ticket)
        }
    }

    /// With rows on screen they stay while the next search runs; otherwise the screen shows the skeleton.
    fn show_loading_unless_rows_are_shown(&mut self) {
        if self.state != SearchState::Loaded || self.results.is_empty() {
            self.state = SearchState::Loading;
        }
    }

    /// The worker answered the search with this number. Stale answers are ignored. Returns the next search to
    /// start when a newer query was waiting.
    pub fn search_done(
        &mut self,
        number: u64,
        result: Result<Vec<SearchResult>, PacmanError>,
    ) -> Option<SearchTicket> {
        let ticket = self.running.take()?;
        if ticket.number != number {
            // Not the search we started: leave everything as it was.
            self.running = Some(ticket);
            return None;
        }
        if self.wanted == Some(number) {
            self.wanted = None;
            self.slow = false;
            self.answered_query = ticket.query.clone();
            match result {
                Ok(found) => {
                    self.results = rank(found, &ticket.query);
                    self.state = SearchState::Loaded;
                }
                Err(error) => {
                    self.results.clear();
                    let text = error_text(&error, Doing::Searching);
                    self.state = match error {
                        PacmanError::BadQuery { .. } => SearchState::BadQuery(text),
                        _ => SearchState::Error(text),
                    };
                }
            }
            self.fix_selection();
        }
        let next = self.pending.take();
        self.running.clone_from(&next);
        next
    }

    /// Position of the selected result in `results()`.
    pub fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.results.iter().position(|r| is_package(r, selected))
    }

    /// The selected result.
    pub fn selected(&self) -> Option<&RankedResult> {
        self.results.get(self.selected_index()?)
    }

    /// The selected package, as `pacman -Si` wants to be asked about it.
    pub fn selected_ref(&self) -> Option<PackageRef> {
        self.selected.clone()
    }

    /// Selects a result by repository and name. Ignored unless it is in the list.
    pub fn select(&mut self, repo: &str, name: &str) {
        if let Some(found) = self
            .results
            .iter()
            .find(|r| r.result.repo == repo && r.result.name == name)
        {
            self.selected = Some(PackageRef::from(&found.result));
        }
    }

    /// Selects the next row. On the last row nothing changes.
    pub fn select_next(&mut self) {
        self.select_page(1);
    }

    /// Selects the previous row. On the first row nothing changes.
    pub fn select_previous(&mut self) {
        self.select_page(-1);
    }

    /// Selects the first row.
    pub fn select_first(&mut self) {
        self.select_page(isize::MIN);
    }

    /// Selects the last row.
    pub fn select_last(&mut self) {
        self.select_page(isize::MAX);
    }

    /// Moves the selection by `delta` rows (negative = up) and stops at the first or last row.
    pub fn select_page(&mut self, delta: isize) {
        let target = moved_selection(self.selected_index(), self.results.len(), delta);
        if let Some(found) = target.and_then(|index| self.results.get(index)) {
            self.selected = Some(PackageRef::from(&found.result));
        }
    }

    /// Keeps the selected package if it is still listed; otherwise selects the first row, or nothing.
    fn fix_selection(&mut self) {
        if self.selected_index().is_none() {
            self.selected = self
                .results
                .first()
                .map(|first| PackageRef::from(&first.result));
        }
    }

    pub fn state(&self) -> &SearchState {
        &self.state
    }

    /// The ranked results of the last answer that was used.
    pub fn results(&self) -> &[RankedResult] {
        &self.results
    }

    /// The query the results (or the error) answer. Differs from `query` while a newer search runs.
    pub fn answered_query(&self) -> &str {
        &self.answered_query
    }

    /// True from the moment a search for the latest text is wanted until its answer arrives.
    pub fn is_searching(&self) -> bool {
        self.wanted.is_some()
    }

    /// Called when a search has taken too long. Ignored unless one is wanted.
    pub fn mark_slow(&mut self) {
        if self.is_searching() {
            self.slow = true;
        }
    }

    pub fn is_slow(&self) -> bool {
        self.slow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn names(ranked: &[RankedResult]) -> Vec<&str> {
        ranked.iter().map(|r| r.result.name.as_str()).collect()
    }

    fn kinds(ranked: &[RankedResult]) -> Vec<MatchKind> {
        ranked.iter().map(|r| r.matched).collect()
    }

    #[test]
    fn the_arch_repositories_are_plain() {
        assert_eq!(repo_kind("core"), RepoKind::Official);
        assert_eq!(repo_kind("extra"), RepoKind::Official);
        assert_eq!(repo_kind("multilib"), RepoKind::Official);
    }

    #[test]
    fn a_repository_ending_in_testing_is_testing() {
        assert_eq!(repo_kind("core-testing"), RepoKind::Testing);
        assert_eq!(repo_kind("extra-testing"), RepoKind::Testing);
        assert_eq!(repo_kind("multilib-testing"), RepoKind::Testing);
        assert_eq!(repo_kind("kde-unstable-testing"), RepoKind::Testing);
    }

    #[test]
    fn every_other_repository_is_third_party() {
        assert_eq!(repo_kind("chaotic-aur"), RepoKind::ThirdParty);
        assert_eq!(repo_kind("my-testing-mirror"), RepoKind::ThirdParty);
        assert_eq!(repo_kind("testing"), RepoKind::ThirdParty);
        assert_eq!(repo_kind("Extra"), RepoKind::ThirdParty);
        assert_eq!(repo_kind(""), RepoKind::ThirdParty);
    }

    #[test]
    fn a_result_carries_its_repository_kind() {
        let ranked = rank(
            vec![
                hit("chaotic-aur", "vlc", "Player"),
                extra("vlc-x", "Player"),
            ],
            "vlc",
        );
        assert_eq!(ranked[0].repo_kind, RepoKind::ThirdParty);
        assert_eq!(ranked[1].repo_kind, RepoKind::Official);
    }

    #[test]
    fn name_matches_come_before_description_matches_before_provides() {
        // pacman's order: a provides-only hit, a description hit, a name hit.
        let ranked = rank(
            vec![
                extra("flac", "Free Lossless Audio Codec"),
                extra("kdenlive", "Video editor built on libvideo"),
                extra("video-tool", "Does things"),
            ],
            "video",
        );
        assert_eq!(names(&ranked), ["video-tool", "kdenlive", "flac"]);
        assert_eq!(
            kinds(&ranked),
            [MatchKind::Name, MatchKind::Description, MatchKind::Provides]
        );
    }

    #[test]
    fn pacmans_order_is_kept_inside_each_group() {
        let ranked = rank(
            vec![
                extra("zz-video", ""),
                extra("c", "a video thing"),
                extra("aa-video", ""),
                extra("b", "another video thing"),
            ],
            "video",
        );
        assert_eq!(names(&ranked), ["zz-video", "aa-video", "c", "b"]);
    }

    #[test]
    fn an_exact_name_comes_first() {
        let ranked = rank(
            vec![
                extra("vlc-plugin-ffmpeg", "More formats"),
                extra("libvlc", "Library"),
                extra("vlc", "The player"),
            ],
            "vlc",
        );
        assert_eq!(names(&ranked), ["vlc", "vlc-plugin-ffmpeg", "libvlc"]);
        assert_eq!(kinds(&ranked), [MatchKind::Name; 3]);
    }

    #[test]
    fn matching_ignores_case_in_the_query_and_in_the_package() {
        let ranked = rank(
            vec![
                extra("vlc-plugin", "Plugin"),
                extra("VLC", "Exact apart from case"),
            ],
            "  VlC ",
        );
        assert_eq!(names(&ranked), ["VLC", "vlc-plugin"]);
        let ranked = rank(vec![extra("player", "A VIDEO player")], "video");
        assert_eq!(kinds(&ranked), [MatchKind::Description]);
    }

    #[test]
    fn with_several_words_all_of_them_must_be_in_the_name_for_a_name_match() {
        let ranked = rank(
            vec![
                extra("vim", "Vi Improved text editor"),
                extra("text-editor-x", "Something"),
                extra("editor", "Edits text"),
                extra("tool", "Nothing related"),
            ],
            "text editor",
        );
        assert_eq!(names(&ranked), ["text-editor-x", "vim", "editor", "tool"]);
        assert_eq!(
            kinds(&ranked),
            [
                MatchKind::Name,
                MatchKind::Description,
                MatchKind::Description,
                MatchKind::Provides
            ]
        );
    }

    #[test]
    fn a_package_that_pacman_matched_only_through_provides_is_marked() {
        // Real finding from task 4.3: `c++` finds flac through libFLAC++.so, which is in neither text.
        let ranked = rank(vec![extra("flac", "Free Lossless Audio Codec")], "c++");
        assert_eq!(kinds(&ranked), [MatchKind::Provides]);
    }

    #[test]
    fn the_same_name_in_two_repositories_keeps_pacmans_order() {
        let ranked = rank(
            vec![
                hit("extra-testing", "vlc", "Test build"),
                extra("vlc", "Player"),
            ],
            "vlc",
        );
        let repos: Vec<&str> = ranked.iter().map(|r| r.result.repo.as_str()).collect();
        assert_eq!(repos, ["extra-testing", "extra"]);
    }

    #[test]
    fn no_results_rank_to_no_results() {
        assert!(rank(Vec::new(), "vlc").is_empty());
    }

    // ---- the model: numbering, stale answers, one search at a time ----

    fn found(names: &[&str]) -> Result<Vec<SearchResult>, PacmanError> {
        Ok(names.iter().map(|name| extra(name, "")).collect())
    }

    fn result_names(model: &SearchModel) -> Vec<&str> {
        model
            .results()
            .iter()
            .map(|r| r.result.name.as_str())
            .collect()
    }

    /// A model whose search box holds `text` and whose search has been asked for.
    fn searching(text: &str) -> (SearchModel, SearchTicket) {
        let mut model = SearchModel::new();
        model.set_query(text);
        let ticket = model
            .start_search()
            .expect("a ticket for a non-blank query");
        (model, ticket)
    }

    fn failed() -> PacmanError {
        PacmanError::Failed {
            code: Some(1),
            stderr: "error: boom".to_string(),
        }
    }

    #[test]
    fn a_new_model_shows_the_start_card() {
        let mut model = SearchModel::new();
        assert_eq!(model.state(), &SearchState::Start);
        assert!(model.results().is_empty());
        assert!(!model.is_searching());
        assert_eq!(model.start_search(), None);
    }

    #[test]
    fn a_blank_query_searches_nothing() {
        let mut model = SearchModel::new();
        model.set_query("  \t ");
        assert_eq!(model.query(), "");
        assert_eq!(model.start_search(), None);
        assert_eq!(model.state(), &SearchState::Start);
        assert!(!model.is_searching());
    }

    #[test]
    fn a_search_gets_a_number_and_the_trimmed_text() {
        let (model, ticket) = searching("  vlc ");
        assert_eq!(
            ticket,
            SearchTicket {
                number: 1,
                query: "vlc".to_string()
            }
        );
        assert_eq!(model.query(), "vlc");
        assert_eq!(model.state(), &SearchState::Loading);
        assert!(model.is_searching());
    }

    #[test]
    fn the_numbers_go_up_with_every_search() {
        let (mut model, first) = searching("vlc");
        assert_eq!(model.search_done(first.number, found(&["vlc"])), None);
        model.set_query("java");
        let second = model.start_search().unwrap();
        assert_eq!(second.number, first.number + 1);
    }

    #[test]
    fn the_answer_is_ranked_and_shown() {
        let (mut model, ticket) = searching("vlc");
        let next = model.search_done(ticket.number, found(&["vlc-plugin", "vlc"]));
        assert_eq!(next, None);
        assert_eq!(model.state(), &SearchState::Loaded);
        assert_eq!(result_names(&model), ["vlc", "vlc-plugin"]);
        assert_eq!(model.answered_query(), "vlc");
        assert!(!model.is_searching());
    }

    #[test]
    fn an_answer_with_no_results_is_still_an_answer() {
        let (mut model, ticket) = searching("spotfy");
        model.search_done(ticket.number, found(&[]));
        assert_eq!(model.state(), &SearchState::Loaded);
        assert!(model.results().is_empty());
        assert_eq!(model.answered_query(), "spotfy");
    }

    #[test]
    fn a_pacman_failure_is_explained_as_a_failed_search() {
        let (mut model, ticket) = searching("vlc");
        model.search_done(ticket.number, Err(failed()));
        let SearchState::Error(text) = model.state() else {
            panic!("expected the Error state, got {:?}", model.state());
        };
        assert_eq!(text.title, "pacman couldn't finish the search");
        assert_eq!(text.technical, "exit code 1: error: boom");
        assert!(model.results().is_empty());
        assert!(!model.is_searching());
    }

    #[test]
    fn a_refused_query_is_shown_as_a_bad_query_and_clears_the_results() {
        let (mut model, first) = searching("vlc");
        model.search_done(first.number, found(&["vlc"]));
        model.set_query("a");
        let second = model.start_search().unwrap();
        model.search_done(
            second.number,
            Err(PacmanError::BadQuery {
                reason: "the search text is too long",
            }),
        );
        let SearchState::BadQuery(text) = model.state() else {
            panic!("expected the BadQuery state, got {:?}", model.state());
        };
        assert_eq!(text.title, "That search can't be used");
        assert!(model.results().is_empty());
    }

    #[test]
    fn trying_again_after_an_error_searches_the_same_text_again() {
        let (mut model, ticket) = searching("vlc");
        model.search_done(ticket.number, Err(failed()));
        let again = model.start_search().unwrap();
        assert_eq!(again.query, "vlc");
        assert_eq!(again.number, ticket.number + 1);
        assert_eq!(model.state(), &SearchState::Loading);
    }

    #[test]
    fn a_new_search_keeps_the_old_rows_on_screen_until_it_answers() {
        let (mut model, first) = searching("vlc");
        model.search_done(first.number, found(&["vlc"]));
        model.set_query("java");
        let second = model.start_search().unwrap();
        assert_eq!(model.state(), &SearchState::Loaded);
        assert_eq!(result_names(&model), ["vlc"]);
        assert_eq!(model.answered_query(), "vlc");
        assert_eq!(model.query(), "java");
        assert!(model.is_searching());
        model.search_done(second.number, found(&["jre"]));
        assert_eq!(result_names(&model), ["jre"]);
        assert_eq!(model.answered_query(), "java");
    }

    #[test]
    fn only_one_search_runs_at_a_time() {
        let (mut model, _first) = searching("a");
        model.set_query("ab");
        assert_eq!(model.start_search(), None);
        model.set_query("abc");
        assert_eq!(model.start_search(), None);
    }

    #[test]
    fn the_answer_to_an_older_query_is_dropped_and_the_waiting_one_runs_next() {
        let (mut model, first) = searching("a");
        model.set_query("ab");
        assert_eq!(model.start_search(), None);
        // The answer for "a" arrives: it must not be shown, and "ab" starts now.
        let next = model.search_done(first.number, found(&["apple"])).unwrap();
        assert_eq!(next.query, "ab");
        assert_eq!(next.number, 2);
        assert_eq!(model.state(), &SearchState::Loading);
        assert!(model.results().is_empty());
        assert!(model.is_searching());
        assert_eq!(model.search_done(next.number, found(&["abiword"])), None);
        assert_eq!(result_names(&model), ["abiword"]);
        assert_eq!(model.answered_query(), "ab");
    }

    #[test]
    fn while_one_search_runs_only_the_newest_waiting_query_is_kept() {
        let (mut model, first) = searching("a");
        for text in ["ab", "abc", "abcd"] {
            model.set_query(text);
            assert_eq!(model.start_search(), None);
        }
        let next = model.search_done(first.number, found(&["x"])).unwrap();
        assert_eq!(next.query, "abcd");
        // Nothing else is waiting: "ab" and "abc" were dropped.
        assert_eq!(model.search_done(next.number, found(&["abcdef"])), None);
        assert_eq!(result_names(&model), ["abcdef"]);
    }

    #[test]
    fn typing_back_to_the_running_query_keeps_its_answer() {
        let (mut model, first) = searching("vlc");
        model.set_query("vlcx");
        assert_eq!(model.start_search(), None);
        model.set_query("vlc");
        assert_eq!(model.start_search(), None);
        // The running search is for exactly what is in the box again: use it, and start nothing else.
        assert_eq!(model.search_done(first.number, found(&["vlc"])), None);
        assert_eq!(result_names(&model), ["vlc"]);
        assert_eq!(model.answered_query(), "vlc");
    }

    #[test]
    fn clearing_the_box_makes_the_running_answer_stale() {
        let (mut model, ticket) = searching("vlc");
        model.set_query("");
        assert_eq!(model.state(), &SearchState::Start);
        assert!(!model.is_searching());
        assert_eq!(model.search_done(ticket.number, found(&["vlc"])), None);
        assert_eq!(model.state(), &SearchState::Start);
        assert!(model.results().is_empty());
    }

    #[test]
    fn clearing_the_box_also_drops_the_waiting_query() {
        let (mut model, first) = searching("a");
        model.set_query("ab");
        assert_eq!(model.start_search(), None);
        model.set_query("");
        assert_eq!(model.search_done(first.number, found(&["apple"])), None);
        assert_eq!(model.state(), &SearchState::Start);
    }

    #[test]
    fn clearing_the_box_removes_the_rows_that_were_shown() {
        let (mut model, ticket) = searching("vlc");
        model.search_done(ticket.number, found(&["vlc"]));
        model.set_query("");
        assert_eq!(model.state(), &SearchState::Start);
        assert!(model.results().is_empty());
        assert_eq!(model.answered_query(), "");
    }

    #[test]
    fn a_stale_failure_is_not_shown() {
        let (mut model, first) = searching("a");
        model.set_query("ab");
        model.start_search();
        let next = model.search_done(first.number, Err(failed()));
        assert!(next.is_some());
        assert_eq!(model.state(), &SearchState::Loading);
    }

    #[test]
    fn an_answer_with_an_unknown_number_is_ignored() {
        let (mut model, ticket) = searching("vlc");
        assert_eq!(model.search_done(99, found(&["nope"])), None);
        assert_eq!(model.state(), &SearchState::Loading);
        // The real answer still works.
        model.search_done(ticket.number, found(&["vlc"]));
        assert_eq!(result_names(&model), ["vlc"]);
    }

    #[test]
    fn a_search_is_slow_only_after_it_is_marked_and_only_while_wanted() {
        let mut model = SearchModel::new();
        model.mark_slow(); // nothing is being searched: ignored
        assert!(!model.is_slow());
        let (mut model, ticket) = searching("vlc");
        assert!(!model.is_slow());
        model.mark_slow();
        assert!(model.is_slow());
        model.search_done(ticket.number, found(&["vlc"]));
        assert!(!model.is_slow());
        model.mark_slow(); // a late timer after the answer: ignored
        assert!(!model.is_slow());
    }

    #[test]
    fn a_new_search_forgets_that_the_last_one_was_slow() {
        let (mut model, first) = searching("vlc");
        model.mark_slow();
        model.search_done(first.number, found(&["vlc"]));
        model.set_query("java");
        model.start_search();
        assert!(!model.is_slow());
    }

    // ---- selection and keys ----

    /// A model that has answered "x" with these names (all name matches, so pacman's order is kept).
    fn loaded(names: &[&str]) -> SearchModel {
        let (mut model, ticket) = searching("x");
        model.search_done(ticket.number, found(names));
        model
    }

    fn selected_name(model: &SearchModel) -> Option<&str> {
        model.selected().map(|r| r.result.name.as_str())
    }

    #[test]
    fn after_an_answer_the_first_row_is_selected() {
        let model = loaded(&["x1", "x2", "x3"]);
        assert_eq!(selected_name(&model), Some("x1"));
        assert_eq!(model.selected_index(), Some(0));
        assert_eq!(
            model.selected_ref(),
            Some(PackageRef {
                repo: "extra".to_string(),
                name: "x1".to_string()
            })
        );
    }

    #[test]
    fn nothing_is_selected_without_results() {
        assert_eq!(selected_name(&SearchModel::new()), None);
        assert_eq!(SearchModel::new().selected_ref(), None);
        assert_eq!(selected_name(&loaded(&[])), None);
        assert_eq!(loaded(&[]).selected_index(), None);
    }

    #[test]
    fn a_result_can_be_selected_by_repository_and_name() {
        let mut model = loaded(&["x1", "x2", "x3"]);
        model.select("extra", "x3");
        assert_eq!(selected_name(&model), Some("x3"));
        assert_eq!(model.selected_index(), Some(2));
    }

    #[test]
    fn selecting_something_that_is_not_listed_is_ignored() {
        let mut model = loaded(&["x1", "x2"]);
        model.select("extra", "nope");
        model.select("core", "x2"); // right name, wrong repository
        assert_eq!(selected_name(&model), Some("x1"));
    }

    #[test]
    fn the_same_name_in_two_repositories_is_two_rows() {
        let (mut model, ticket) = searching("x");
        model.search_done(
            ticket.number,
            Ok(vec![hit("extra", "x1", ""), hit("extra-testing", "x1", "")]),
        );
        model.select("extra-testing", "x1");
        assert_eq!(model.selected_index(), Some(1));
        assert_eq!(model.selected_ref().unwrap().repo, "extra-testing");
    }

    #[test]
    fn next_and_previous_move_one_row_and_stop_at_the_ends() {
        let mut model = loaded(&["x1", "x2", "x3"]);
        model.select_previous();
        assert_eq!(selected_name(&model), Some("x1"));
        model.select_next();
        model.select_next();
        assert_eq!(selected_name(&model), Some("x3"));
        model.select_next();
        assert_eq!(selected_name(&model), Some("x3"));
        model.select_previous();
        assert_eq!(selected_name(&model), Some("x2"));
    }

    #[test]
    fn first_last_and_pages_jump_and_stop_at_the_ends() {
        let mut model = loaded(&["x1", "x2", "x3", "x4", "x5"]);
        model.select_last();
        assert_eq!(selected_name(&model), Some("x5"));
        model.select_first();
        assert_eq!(selected_name(&model), Some("x1"));
        model.select_page(3);
        assert_eq!(selected_name(&model), Some("x4"));
        model.select_page(-2);
        assert_eq!(selected_name(&model), Some("x2"));
        model.select_page(100);
        assert_eq!(selected_name(&model), Some("x5"));
        model.select_page(isize::MIN);
        assert_eq!(selected_name(&model), Some("x1"));
    }

    #[test]
    fn moving_with_no_rows_does_nothing() {
        let mut model = loaded(&[]);
        model.select_next();
        model.select_previous();
        model.select_first();
        model.select_last();
        model.select_page(5);
        assert_eq!(selected_name(&model), None);
    }

    #[test]
    fn a_new_answer_keeps_the_selection_if_that_package_is_still_listed() {
        let mut model = loaded(&["x1", "x2", "x3"]);
        model.select("extra", "x2");
        // Search "x" again (try again): the answer lists x2 in a different place.
        let ticket = model.start_search().unwrap();
        model.search_done(ticket.number, found(&["x3", "x2"]));
        assert_eq!(selected_name(&model), Some("x2"));
        assert_eq!(model.selected_index(), Some(1));
    }

    #[test]
    fn a_new_answer_without_the_selected_package_selects_the_first_row() {
        let mut model = loaded(&["x1", "x2", "x3"]);
        model.select("extra", "x2");
        model.set_query("x9");
        let ticket = model.start_search().unwrap();
        model.search_done(ticket.number, found(&["x9a", "x9b"]));
        assert_eq!(selected_name(&model), Some("x9a"));
    }

    #[test]
    fn the_selection_stays_while_a_later_search_runs_and_goes_when_the_box_is_cleared() {
        let mut model = loaded(&["x1", "x2"]);
        model.select("extra", "x2");
        model.set_query("y");
        model.start_search();
        assert_eq!(selected_name(&model), Some("x2"));
        model.set_query("");
        assert_eq!(selected_name(&model), None);
    }

    #[test]
    fn an_error_leaves_nothing_selected() {
        let mut model = loaded(&["x1"]);
        model.set_query("y");
        let ticket = model.start_search().unwrap();
        model.search_done(ticket.number, Err(failed()));
        assert_eq!(selected_name(&model), None);
    }
}
