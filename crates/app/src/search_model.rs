//! View model for the Search screen: the query, the ranked results, the selection and the details.
//! Plain Rust, no Slint, no threads, no clock. It never runs pacman; it is handed pacman's answers.

use tidewise_pacman::SearchResult;

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

/// Which kind of repository this is, from its name.
pub fn repo_kind(_repo: &str) -> RepoKind {
    // Stub for the red commit.
    RepoKind::Official
}

/// Puts pacman's results in the order people expect: an exact name first, then name matches, then
/// description matches, then provides-only matches. Inside each group pacman's own order is kept.
pub fn rank(results: Vec<SearchResult>, _query: &str) -> Vec<RankedResult> {
    // Stub for the red commit: pacman's order, everything a name match.
    results
        .into_iter()
        .map(|result| RankedResult {
            repo_kind: repo_kind(&result.repo),
            result,
            matched: MatchKind::Name,
        })
        .collect()
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
}
