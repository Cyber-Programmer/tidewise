//! `summary`: the one-line text pkexec shows in the password dialog (decision 0005, threat T-02).
//!
//! The app puts this text in argv; the helper recomputes it from the request it validated and refuses on any
//! difference. So the function is deterministic, ASCII only, and capped in length. It describes the request;
//! it is not a hash: it lists the targets (and, when they fit, the chosen providers), counts the confirmed
//! list, and never shows versions. Package names only: argv is visible to every user in `ps`.

use crate::Request;

/// Longest summary, in bytes. Longer lists are cut with "and N more".
pub const MAX_SUMMARY_BYTES: usize = 200;

/// The text for `request`. Works on any request, validated or not: whatever it holds, the result is printable
/// ASCII (anything else becomes `?`) and at most `MAX_SUMMARY_BYTES` long.
///
/// STUB (red commit): returns nothing.
pub fn summary(_request: &Request) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Action, Install, PROTOCOL_VERSION, PackageRef, ProviderChoice, TransactionItem,
        UpdateSystem,
    };

    const INSTALL: &str = include_str!("../../../tests/fixtures/helper-protocol/install.json");
    const UPDATE: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
    const BOTH: &str =
        include_str!("../../../tests/fixtures/helper-protocol/update-system-and-install.json");

    fn from_json(text: &str) -> Request {
        serde_json::from_str(text).unwrap()
    }

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    fn choice(dependency: &str, repo: &str, name: &str) -> ProviderChoice {
        ProviderChoice {
            dependency: dependency.to_string(),
            chosen: pkg(repo, name),
        }
    }

    /// `count` confirmed lines (their content does not matter to the summary, only how many).
    fn lines(count: usize) -> Vec<TransactionItem> {
        (0..count)
            .map(|n| TransactionItem {
                repo: "extra".to_string(),
                name: format!("p{n}"),
                version: "1-1".to_string(),
            })
            .collect()
    }

    fn install(targets: Vec<PackageRef>, providers: Vec<ProviderChoice>, count: usize) -> Request {
        Request {
            protocol_version: PROTOCOL_VERSION,
            action: Action::Install(Install {
                targets,
                providers,
                transaction: lines(count),
            }),
        }
    }

    fn update(count: usize) -> Request {
        Request {
            protocol_version: PROTOCOL_VERSION,
            action: Action::UpdateSystem(UpdateSystem {
                transaction: lines(count),
            }),
        }
    }

    fn total(count: usize) -> String {
        if count == 1 {
            " (1 package)".to_string()
        } else {
            format!(" ({count} packages)")
        }
    }

    fn is_printable_ascii(text: &str) -> bool {
        text.bytes().all(|b| (0x20..=0x7e).contains(&b))
    }

    #[test]
    fn the_sample_requests_read_like_this() {
        assert_eq!(
            summary(&from_json(INSTALL)),
            "install extra/dbeaver with extra/jre-openjdk (5 packages)"
        );
        assert_eq!(summary(&from_json(UPDATE)), "update system (3 packages)");
        assert_eq!(
            summary(&from_json(BOTH)),
            "update system and install extra/vlc (2 packages)"
        );
    }

    #[test]
    fn exact_text_for_each_shape() {
        assert_eq!(summary(&update(143)), "update system (143 packages)");
        assert_eq!(summary(&update(1)), "update system (1 package)");
        assert_eq!(
            summary(&install(
                vec![pkg("extra", "dbeaver"), pkg("extra", "gimp")],
                vec![],
                14
            )),
            "install extra/dbeaver extra/gimp (14 packages)"
        );
        assert_eq!(
            summary(&install(
                vec![pkg("extra", "a")],
                vec![choice("x", "extra", "b"), choice("y", "core", "c")],
                3
            )),
            "install extra/a with extra/b core/c (3 packages)"
        );
    }

    #[test]
    fn the_same_request_gives_the_same_text() {
        for text in [INSTALL, UPDATE, BOTH] {
            let request = from_json(text);
            assert_eq!(summary(&request), summary(&request.clone()));
        }
    }

    #[test]
    fn versions_and_confirmed_names_are_not_in_the_text_only_their_count() {
        let mut changed = from_json(INSTALL);
        if let Action::Install(install) = &mut changed.action {
            for line in &mut install.transaction {
                line.version = "9.9.9-9".to_string();
            }
        }
        assert_eq!(summary(&changed), summary(&from_json(INSTALL)));
    }

    #[test]
    fn requests_that_differ_in_what_the_user_sees_read_differently() {
        let base = install(vec![pkg("extra", "a"), pkg("extra", "b")], vec![], 4);
        let other_repo = install(vec![pkg("multilib", "a"), pkg("extra", "b")], vec![], 4);
        let other_order = install(vec![pkg("extra", "b"), pkg("extra", "a")], vec![], 4);
        let other_count = install(vec![pkg("extra", "a"), pkg("extra", "b")], vec![], 5);
        let one_provider = install(
            vec![pkg("extra", "a"), pkg("extra", "b")],
            vec![choice("x", "extra", "c")],
            4,
        );
        let other_provider = install(
            vec![pkg("extra", "a"), pkg("extra", "b")],
            vec![choice("x", "extra", "d")],
            4,
        );
        let both = Request {
            protocol_version: PROTOCOL_VERSION,
            action: match base.action.clone() {
                Action::Install(i) => Action::UpdateSystemAndInstall(i),
                other => other,
            },
        };
        let all = [
            &base,
            &other_repo,
            &other_order,
            &other_count,
            &one_provider,
            &other_provider,
            &both,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(summary(a), summary(b));
            }
        }
    }

    #[test]
    fn a_single_target_is_cut_exactly_at_the_cap() {
        // Walk the name length across the 200-byte line: up to 200 the full text is used, past it the
        // target is replaced by "..." and counted in "and 1 more".
        for len in 1..=300 {
            let name = "a".repeat(len);
            let full = format!("install extra/{name} (2 packages)");
            let text = summary(&install(vec![pkg("extra", &name)], vec![], 2));
            assert!(text.len() <= MAX_SUMMARY_BYTES, "len {len}: {}", text.len());
            if full.len() <= MAX_SUMMARY_BYTES {
                assert_eq!(text, full, "len {len}");
            } else {
                assert_eq!(text, "install ... and 1 more (2 packages)", "len {len}");
            }
        }
    }

    #[test]
    fn and_n_more_counts_the_targets_that_did_not_fit() {
        for count in 1..=100 {
            let targets: Vec<PackageRef> = (0..count)
                .map(|n| pkg("extra", &format!("package-with-a-long-name-{n:03}")))
                .collect();
            let expected = |shown: usize| {
                let mut text = String::from("install");
                if shown == 0 && count > 0 {
                    text.push_str(" ...");
                }
                for target in targets.iter().take(shown) {
                    text.push_str(&format!(" {}/{}", target.repo, target.name));
                }
                if shown < count {
                    text.push_str(&format!(" and {} more", count - shown));
                }
                text.push_str(&total(count));
                text
            };
            let most_that_fit = (0..=count)
                .rev()
                .find(|&shown| expected(shown).len() <= MAX_SUMMARY_BYTES)
                .unwrap();
            let text = summary(&install(targets.clone(), vec![], count));
            assert_eq!(text, expected(most_that_fit), "count {count}");
            assert!(text.len() <= MAX_SUMMARY_BYTES);
            if most_that_fit < count {
                assert!(expected(most_that_fit + 1).len() > MAX_SUMMARY_BYTES);
            }
        }
    }

    #[test]
    fn chosen_providers_are_listed_when_all_of_them_fit_and_left_out_when_not() {
        // Walk the provider name length across the cap.
        for len in 1..=300 {
            let name = "b".repeat(len);
            let with = format!("install extra/a with extra/{name} (2 packages)");
            let without = "install extra/a (2 packages)";
            let text = summary(&install(
                vec![pkg("extra", "a")],
                vec![choice("x", "extra", &name)],
                2,
            ));
            if with.len() <= MAX_SUMMARY_BYTES {
                assert_eq!(text, with, "len {len}");
            } else {
                assert_eq!(text, without, "len {len}");
            }
        }
    }

    #[test]
    fn providers_are_all_or_nothing() {
        let providers: Vec<ProviderChoice> = (0..30)
            .map(|n| choice(&format!("d{n}"), "extra", &format!("provider-{n}")))
            .collect();
        let text = summary(&install(vec![pkg("extra", "a")], providers, 31));
        assert_eq!(text, "install extra/a (31 packages)");
    }

    #[test]
    fn providers_are_left_out_when_the_targets_were_cut() {
        let targets: Vec<PackageRef> = (0..50)
            .map(|n| pkg("extra", &format!("package-with-a-long-name-{n:03}")))
            .collect();
        let text = summary(&install(targets, vec![choice("x", "extra", "p")], 60));
        assert!(text.contains(" more (60 packages)"), "{text}");
        assert!(!text.contains(" with "), "{text}");
    }

    #[test]
    fn update_system_and_install_lists_providers_the_same_way() {
        let request = Request {
            protocol_version: PROTOCOL_VERSION,
            action: Action::UpdateSystemAndInstall(Install {
                targets: vec![pkg("extra", "dbeaver")],
                providers: vec![choice("java-runtime", "extra", "jre-openjdk")],
                transaction: lines(150),
            }),
        };
        assert_eq!(
            summary(&request),
            "update system and install extra/dbeaver with extra/jre-openjdk (150 packages)"
        );
    }

    #[test]
    fn whatever_the_request_holds_the_text_is_printable_ascii_and_capped() {
        let hostile = [
            "a\0b",
            "a\nb",
            "a\rb",
            "\u{1b}[31mred",
            "\u{202e}rtl",
            "\u{430}pp",
            "\u{ff41}\u{ff50}\u{ff50}",
            "caf\u{e9}",
            "\u{1f600}",
            "tab\there",
            "\u{7f}",
            "",
        ];
        let big = "x".repeat(1_000_000);
        for name in hostile.iter().copied().chain(std::iter::once(big.as_str())) {
            let requests = [
                install(vec![pkg(name, name)], vec![choice(name, name, name)], 1),
                install(vec![pkg("extra", name), pkg(name, "b")], vec![], 0),
                Request {
                    protocol_version: PROTOCOL_VERSION,
                    action: Action::UpdateSystemAndInstall(Install {
                        targets: vec![pkg(name, name)],
                        providers: vec![],
                        transaction: vec![],
                    }),
                },
                update(0),
            ];
            for request in &requests {
                let text = summary(request);
                assert!(is_printable_ascii(&text), "{name:?} gave {text:?}");
                assert!(text.len() <= MAX_SUMMARY_BYTES, "{name:?}");
                assert!(!text.is_empty());
            }
        }
    }

    #[test]
    fn non_ascii_becomes_a_question_mark_one_per_character() {
        let text = summary(&install(vec![pkg("extra", "caf\u{e9}\u{430}")], vec![], 2));
        assert_eq!(text, "install extra/caf?? (2 packages)");
    }

    #[test]
    fn empty_lists_and_huge_lists_do_not_break_it() {
        assert_eq!(summary(&install(vec![], vec![], 0)), "install (0 packages)");
        let many: Vec<PackageRef> = (0..100_000)
            .map(|n| pkg("extra", &format!("p{n}")))
            .collect();
        let text = summary(&install(many, vec![], 100_000));
        assert!(text.len() <= MAX_SUMMARY_BYTES);
        assert!(text.ends_with(" (100000 packages)"));
        assert!(text.contains(" more"));
    }
}
