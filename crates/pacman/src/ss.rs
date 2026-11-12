//! Parser for `pacman -Ss`: a header line and an indented description line per package.

use crate::ParseError;

/// One package found by `pacman -Ss`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// The repository exactly as pacman printed it (`core`, `extra`, `chaotic-aur`, ...).
    pub repo: String,
    pub name: String,
    /// The version in the repository.
    pub version: String,
    /// Empty when the package is in no group.
    pub groups: Vec<String>,
    /// `Some(version)` when installed: the same as `version`, or an older/newer one for
    /// `[installed: other-version]`. `None` when not installed.
    pub installed_version: Option<String>,
    pub description: String,
}

impl SearchResult {
    /// True for the repositories Arch itself ships (`core`, `extra`, `multilib`).
    pub fn is_official(&self) -> bool {
        false
    }
}

/// Parses the output of `LC_ALL=C pacman -Ss -- ...`, keeping pacman's own order.
pub fn parse_ss(_text: &str) -> Result<Vec<SearchResult>, ParseError> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../tests/fixtures/pacman/ss-sample.txt");
    const THIRD_PARTY: &str = include_str!("../../../tests/fixtures/pacman/ss-third-party.txt");
    const TRUNCATED: &str = include_str!("../../../tests/fixtures/pacman/ss-broken-truncated.txt");

    fn find<'a>(results: &'a [SearchResult], name: &str) -> &'a SearchResult {
        results
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("{name} not in results"))
    }

    fn names(results: &[SearchResult]) -> Vec<&str> {
        results.iter().map(|r| r.name.as_str()).collect()
    }

    /// Runs `parse_ss` and returns its error, with a clear message if it succeeded.
    fn expect_error(text: &str) -> ParseError {
        match parse_ss(text) {
            Err(error) => error,
            Ok(results) => panic!("expected an error, got {results:?}"),
        }
    }

    #[test]
    fn sample_gives_seven_results_in_pacmans_order() {
        let results = parse_ss(SAMPLE).unwrap();
        assert_eq!(
            names(&results),
            [
                "tzdata",
                "7zip",
                "aalib",
                "alsa-card-profiles",
                "baobab",
                "gnome-calculator",
                "zsh"
            ]
        );
    }

    #[test]
    fn plain_not_installed_package() {
        let results = parse_ss(SAMPLE).unwrap();
        assert_eq!(
            *find(&results, "aalib"),
            SearchResult {
                repo: "extra".into(),
                name: "aalib".into(),
                version: "1.4rc5-19".into(),
                groups: vec![],
                installed_version: None,
                description: "ASCII art graphic library".into(),
            }
        );
    }

    #[test]
    fn installed_without_a_version_means_same_version() {
        let results = parse_ss(SAMPLE).unwrap();
        let zsh = find(&results, "zsh");
        assert_eq!(zsh.version, "5.9.2-1");
        assert_eq!(zsh.installed_version.as_deref(), Some("5.9.2-1"));
        assert!(zsh.description.ends_with("for UNIX"));
    }

    #[test]
    fn installed_other_version_keeps_that_version() {
        let results = parse_ss(SAMPLE).unwrap();
        let tzdata = find(&results, "tzdata");
        assert_eq!(tzdata.repo, "core");
        assert_eq!(tzdata.version, "2026f-1");
        assert_eq!(tzdata.installed_version.as_deref(), Some("2026e-1"));
    }

    #[test]
    fn epoch_versions_are_kept_whole() {
        let results = parse_ss(SAMPLE).unwrap();
        let alsa = find(&results, "alsa-card-profiles");
        assert_eq!(alsa.version, "1:1.6.11-1");
        assert_eq!(alsa.installed_version.as_deref(), Some("1:1.6.9-1"));
    }

    #[test]
    fn groups_with_and_without_installed() {
        let results = parse_ss(SAMPLE).unwrap();
        let baobab = find(&results, "baobab");
        assert_eq!(baobab.groups, ["gnome"]);
        assert_eq!(baobab.installed_version.as_deref(), Some("50.0-1"));
        let calc = find(&results, "gnome-calculator");
        assert_eq!(calc.groups, ["gnome"]);
        assert_eq!(calc.installed_version, None);
    }

    #[test]
    fn third_party_repos_are_kept_and_labelled() {
        let results = parse_ss(THIRD_PARTY).unwrap();
        let repos: Vec<&str> = results.iter().map(|r| r.repo.as_str()).collect();
        assert_eq!(
            repos,
            ["extra", "chaotic-aur", "chaotic-aur", "my repo.x", "core"]
        );
    }

    #[test]
    fn two_groups_and_installed_other_version() {
        let results = parse_ss(THIRD_PARTY).unwrap();
        let tools = find(&results, "demo-tools");
        assert_eq!(tools.groups, ["demo-group", "other-group"]);
        assert_eq!(tools.installed_version.as_deref(), Some("3.9-2"));
        assert_eq!(tools.description, "Tools (with groups) for the demo");
    }

    #[test]
    fn keeps_pacmans_order_and_does_not_sort() {
        // The fixture is deliberately not alphabetical and not grouped by repo.
        let results = parse_ss(THIRD_PARTY).unwrap();
        assert_eq!(
            names(&results),
            ["zsh", "tidewise-demo", "demo-tools", "odd-one", "acl"]
        );
    }

    #[test]
    fn is_official_only_for_core_extra_multilib() {
        let results = parse_ss(THIRD_PARTY).unwrap();
        let official: Vec<bool> = results.iter().map(SearchResult::is_official).collect();
        assert_eq!(official, [true, false, false, false, true]);
        for repo in ["core", "extra", "multilib"] {
            let text = format!("{repo}/foo 1-1\n    d\n");
            assert!(parse_ss(&text).unwrap()[0].is_official(), "{repo}");
        }
        // Case matters: pacman repo names are case-sensitive, so "Extra" is a different repo.
        assert!(!parse_ss("Extra/foo 1-1\n    d\n").unwrap()[0].is_official());
    }

    #[test]
    fn repo_rule_is_pacmans_not_stricter() {
        // pacman only needs a non-empty name without '/'. Spaces, dots, capitals and non-ASCII are legal.
        for repo in ["my repo.x", "Chaotic_AUR", "repo-1", "dépôt", "a"] {
            let text = format!("{repo}/foo 1-1\n    d\n");
            assert_eq!(parse_ss(&text).unwrap()[0].repo, repo, "{repo}");
        }
    }

    #[test]
    fn empty_description_line_is_allowed() {
        // A package with no description still gets an indented (blank) line.
        let results = parse_ss("extra/foo 1-1\n    \n").unwrap();
        assert_eq!(results[0].description, "");
    }

    #[test]
    fn empty_input_is_empty_list() {
        assert_eq!(parse_ss("").unwrap(), Vec::<SearchResult>::new());
    }

    #[test]
    fn truncated_header_is_an_error() {
        let error = expect_error(TRUNCATED);
        assert!(
            matches!(&error, ParseError::BadValue { package: Some(p), .. } if p == "alsa-card-profiles"),
            "got {error:?}"
        );
    }

    #[test]
    fn header_without_description_is_missing_field() {
        for text in [
            "extra/foo 1-1\n",
            "extra/foo 1-1",
            "extra/foo 1-1\nextra/bar 2-1\n    d\n",
        ] {
            let error = expect_error(text);
            assert!(
                matches!(&error, ParseError::MissingField { package: Some(p), field: "Description" } if p == "foo"),
                "{text:?}: {error:?}"
            );
        }
    }

    #[test]
    fn bad_package_names_are_rejected() {
        for name in ["-dd", ".hidden", "foo;rm", "fo$o", "café", "a\tb"] {
            let text = format!("extra/{name} 1-1\n    d\n");
            let error = expect_error(&text);
            assert!(
                matches!(&error, ParseError::BadValue { field: "Name", .. }),
                "{name:?}: {error:?}"
            );
        }
    }

    #[test]
    fn bad_repos_are_rejected() {
        // Empty name; and a line with no '/' at all is not a header.
        let error = expect_error("/foo 1-1\n    d\n");
        assert!(
            matches!(
                &error,
                ParseError::BadValue {
                    field: "Repository",
                    ..
                }
            ),
            "{error:?}"
        );
        let error = expect_error("foo 1-1\n    d\n");
        assert!(
            matches!(&error, ParseError::MalformedLine { line_number: 1, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn missing_or_odd_version_is_rejected() {
        let error = expect_error("extra/foo\n    d\n");
        assert!(
            matches!(&error, ParseError::MalformedLine { line_number: 1, .. }),
            "{error:?}"
        );
        let error = expect_error("extra/foo  [installed]\n    d\n");
        assert!(
            matches!(
                &error,
                ParseError::BadValue {
                    field: "Version",
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn unknown_or_unclosed_extras_are_rejected() {
        for extras in [
            "(gnome",
            "[installed",
            "[installed: ]",
            "[installed:x]",
            "[installed] junk",
            "[maybe]",
            "junk",
            "[installed] (gnome)",
        ] {
            let text = format!("extra/foo 1-1 {extras}\n    d\n");
            let error = expect_error(&text);
            assert!(
                matches!(&error, ParseError::BadValue { package: Some(p), .. } if p == "foo"),
                "{extras:?}: {error:?}"
            );
        }
    }

    #[test]
    fn description_with_no_header_is_malformed() {
        let error = expect_error("    orphan description\n");
        assert!(
            matches!(&error, ParseError::MalformedLine { line_number: 1, .. }),
            "{error:?}"
        );
        // A second indented line after a finished entry has no header either.
        let error = expect_error("extra/foo 1-1\n    d\n    more\n");
        assert!(
            matches!(&error, ParseError::MalformedLine { line_number: 3, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn blank_line_is_malformed() {
        let error = expect_error("extra/foo 1-1\n    d\n\nextra/bar 1-1\n    d\n");
        assert!(
            matches!(&error, ParseError::MalformedLine { line_number: 3, .. }),
            "{error:?}"
        );
    }
}
