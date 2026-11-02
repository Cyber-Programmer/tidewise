//! Ranking for the search box (Ctrl K): a small in-memory index of apps, processes, actions and
//! settings, matched by plain words. No UI, no I/O. Home files are not indexed (see
//! `docs/questions.md`, R7-Q12).
//!
//! Every word of the query must match somewhere (title, keywords or description); better places
//! score higher: the whole title, its start, the start of one of its words, anywhere in it, the
//! keywords, the description, and finally letters in order (`frfx` finds Firefox).

/// The sections of the result list, in the order they are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Apps,
    Processes,
    Actions,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub group: Group,
    pub title: String,
    /// The second line: where it is, how big, what it does.
    pub meta: String,
    /// Words people might type for it ("free space junk cache"), space separated.
    pub keywords: String,
    /// What to open; opaque here.
    pub key: String,
}

/// Longest query that is looked at (the rest is ignored).
pub const MAX_QUERY_CHARS: usize = 100;
/// Most results shown for one group, and in all.
pub const PER_GROUP: usize = 5;
pub const TOTAL: usize = 12;

/// How well `query` matches `item`: `None` when some word of the query matches nothing.
pub fn score(_query: &str, _item: &Item) -> Option<u32> {
    None
}

/// The results for `query`, grouped in display order, best first inside a group. An empty query
/// gives the suggestions: the first few actions and settings.
pub fn search(_items: &[Item], _query: &str) -> Vec<usize> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(group: Group, title: &str, meta: &str, keywords: &str) -> Item {
        Item {
            group,
            title: title.into(),
            meta: meta.into(),
            keywords: keywords.into(),
            key: format!("{group:?}:{title}"),
        }
    }

    fn sample() -> Vec<Item> {
        vec![
            item(Group::Apps, "Firefox", "System app · 248 MB", "browser web"),
            item(Group::Apps, "Steam", "Flatpak · 1.4 GB", "games"),
            item(
                Group::Apps,
                "LibreOffice",
                "System app · 580 MB",
                "office documents",
            ),
            item(Group::Apps, "Spotify", "Snap · 290 MB", "music"),
            item(
                Group::Processes,
                "Firefox",
                "Running · 14 processes · 2.1 GB memory",
                "browser memory",
            ),
            item(
                Group::Processes,
                "Steam",
                "Running · downloading",
                "games network",
            ),
            item(
                Group::Actions,
                "Clean up junk files",
                "12.6 GB found",
                "free space junk cache clean",
            ),
            item(
                Group::Actions,
                "See what's using space",
                "Storage",
                "free space disk full big files",
            ),
            item(
                Group::Actions,
                "Stop apps opening at login",
                "Startup apps",
                "startup boot slow login",
            ),
            item(
                Group::Actions,
                "See what is slowing my PC",
                "Performance",
                "slow memory cpu hot laggy",
            ),
            item(
                Group::Settings,
                "Theme",
                "Appearance",
                "dark light follow system",
            ),
            item(
                Group::Settings,
                "Check for updates",
                "Privacy & updates",
                "version update",
            ),
        ]
    }

    fn titles(items: &[Item], hits: &[usize]) -> Vec<String> {
        hits.iter()
            .map(|&i| format!("{:?}:{}", items[i].group, items[i].title))
            .collect()
    }

    #[test]
    fn better_places_score_higher() {
        let t = |title: &str, meta: &str, kw: &str| item(Group::Apps, title, meta, kw);
        let q = "fire";
        let scores = [
            score(q, &t("Fire", "", "")),                // the whole title
            score(q, &t("Firefox", "", "")),             // the start of the title
            score(q, &t("Open Firefox", "", "")),        // the start of a word
            score(q, &t("Backfired", "", "")),           // inside a word
            score(q, &t("Browser", "", "fire web")),     // a keyword
            score(q, &t("Browser", "made by fire", "")), // the description
        ];
        let got: Vec<u32> = scores
            .iter()
            .map(|s| s.expect("every one matches"))
            .collect();
        assert!(got.windows(2).all(|w| w[0] > w[1]), "{got:?}");
    }

    #[test]
    fn case_and_spaces_do_not_matter() {
        let i = item(Group::Apps, "Firefox", "", "");
        assert_eq!(score("FIREFOX", &i), score("firefox", &i));
        assert_eq!(score("  firefox  ", &i), score("firefox", &i));
        assert!(score("ÉCOLE", &item(Group::Apps, "école", "", "")).is_some());
    }

    #[test]
    fn every_word_must_match_somewhere() {
        let items = sample();
        let hits = search(&items, "dark theme");
        assert_eq!(titles(&items, &hits), ["Settings:Theme"]);
        let hits = search(&items, "firefox mem");
        assert_eq!(
            titles(&items, &hits),
            ["Processes:Firefox"],
            "only the process says memory"
        );
        assert!(search(&items, "firefox zebra").is_empty());
    }

    #[test]
    fn letters_in_order_find_a_title_but_only_from_three_letters() {
        let i = item(Group::Apps, "Firefox", "", "");
        assert!(score("frfx", &i).is_some());
        assert!(score("fx", &i).is_none(), "two letters are too loose");
        assert!(score("xf", &i).is_none(), "wrong order");
        let fuzzy = score("frfx", &i).unwrap();
        let plain = score("fire", &i).unwrap();
        assert!(fuzzy < plain, "a real match beats letters in order");
    }

    #[test]
    fn plain_language_finds_the_actions() {
        let items = sample();
        let free = titles(&items, &search(&items, "free space"));
        // "space" is in the title of one and only a keyword of the other.
        assert_eq!(
            free,
            [
                "Actions:See what's using space",
                "Actions:Clean up junk files"
            ]
        );
        let slow = titles(&items, &search(&items, "slow"));
        assert!(
            slow.contains(&"Actions:See what is slowing my PC".to_string()),
            "{slow:?}"
        );
        assert!(
            slow.contains(&"Actions:Stop apps opening at login".to_string()),
            "{slow:?}"
        );
        let startup = titles(&items, &search(&items, "startup"));
        assert_eq!(startup[0], "Actions:Stop apps opening at login");
    }

    #[test]
    fn groups_come_in_display_order_and_the_best_match_is_first_inside_one() {
        let items = sample();
        let hits = search(&items, "fire");
        assert_eq!(titles(&items, &hits), ["Apps:Firefox", "Processes:Firefox"]);
        let hits = search(&items, "e");
        let groups: Vec<_> = hits.iter().map(|&i| items[i].group).collect();
        assert!(groups.windows(2).all(|w| w[0] <= w[1]), "{groups:?}");
        // Inside a group: the better match first, then by title.
        let mut more = sample();
        more.push(item(Group::Apps, "Fire Alarm", "", ""));
        more.push(item(Group::Apps, "Backfire", "", ""));
        let got = titles(&more, &search(&more, "fire"));
        assert_eq!(
            got[..3],
            ["Apps:Fire Alarm", "Apps:Firefox", "Apps:Backfire"].map(String::from)
        );
    }

    #[test]
    fn a_group_shows_at_most_five_and_the_list_at_most_twelve() {
        let many: Vec<Item> = (0..40)
            .map(|i| {
                item(
                    if i % 2 == 0 {
                        Group::Apps
                    } else {
                        Group::Processes
                    },
                    &format!("tool {i:02}"),
                    "",
                    "",
                )
            })
            .collect();
        let hits = search(&many, "tool");
        assert_eq!(
            hits.iter()
                .filter(|&&i| many[i].group == Group::Apps)
                .count(),
            PER_GROUP
        );
        assert_eq!(hits.len(), 2 * PER_GROUP);
        let four: Vec<Item> = [
            Group::Apps,
            Group::Processes,
            Group::Actions,
            Group::Settings,
        ]
        .iter()
        .flat_map(|g| (0..5).map(move |i| item(*g, &format!("tool {i}"), "", "")))
        .collect();
        assert_eq!(search(&four, "tool").len(), TOTAL);
    }

    #[test]
    fn an_empty_query_suggests_actions_and_settings_only() {
        let items = sample();
        for q in ["", "   "] {
            let hits = search(&items, q);
            assert!(!hits.is_empty() && hits.len() <= 6);
            assert!(hits
                .iter()
                .all(|&i| matches!(items[i].group, Group::Actions | Group::Settings)));
        }
    }

    #[test]
    fn nonsense_and_huge_queries_are_safe_and_bounded() {
        let items = sample();
        assert!(search(&items, "zzzzzz").is_empty());
        for q in [
            "(",
            "[a-",
            ".*",
            "\\",
            "%00",
            "\u{0}",
            "\n\t",
            "🙂🙂🙂",
            &"a".repeat(100_000),
            &"firefox ".repeat(10_000),
        ] {
            let hits = search(&items, q);
            assert!(hits.len() <= TOTAL, "{q:?}");
        }
        // Only the first MAX_QUERY_CHARS count: a long query that starts with a match still matches.
        let long = format!("firefox{}", " ".repeat(500));
        assert!(!search(&items, &long).is_empty());
        assert!(search(&[], "firefox").is_empty());
    }

    #[test]
    fn the_same_input_gives_the_same_order() {
        let items = sample();
        assert_eq!(search(&items, "s"), search(&items, "s"));
    }
}
