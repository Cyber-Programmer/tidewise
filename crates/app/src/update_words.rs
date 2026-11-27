//! What the "Update system" cards say (WBS 6.5). Plain strings, no Slint.

use tidewise_helper_protocol::ErrorKind;
use tidewise_pacman::{Conflict, HeldBlock, PendingUpdate, ProviderPick, Replacement};

/// Why the review offers no Update button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateBlock {
    /// A held package would have to be upgraded too: pacman upgrades nothing.
    Held(HeldBlock),
    /// The real run would abort.
    Conflict(Vec<Conflict>),
    /// pacman would choose a package by itself; the helper would refuse (golden rule 10).
    Pick(Vec<ProviderPick>),
    /// The list stands for a replacement, which Tidewise does not do yet (WBS 7.5).
    Replacement(Vec<Replacement>),
}

pub const NOTHING_REMOVED: &str = "Nothing will be removed.";
/// Rule 13, partly: the address as text, the user reads it (no network of our own, rule 11).
pub const NEWS_LINE: &str =
    "Before updating, check Arch news for steps you must do by hand: archlinux.org/news";
pub const CHECK_FAILED: &str = "Tidewise couldn't check for updates";
pub const UP_TO_DATE: &str = "Your system is up to date";
pub const LISTS_CHANGED: &str =
    "The package lists changed since you looked. Tidewise got the new list.";
pub const RESTART_LINE: &str =
    "A new kernel or core system part was installed. Restart when convenient.";

/// The official kernels. Not `linux-firmware`, headers or `linux-api-headers`: they need no restart by themselves.
const KERNELS: [&str; 6] = [
    "linux",
    "linux-lts",
    "linux-zen",
    "linux-hardened",
    "linux-rt",
    "linux-rt-lts",
];

/// The box that lists what `IgnorePkg` holds back.
pub const HELD_TITLE: &str = "Held back by your pacman.conf";
pub const HELD_WARNING: &str = "A held package can stop other updates from working.";

/// What a blocked card says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWords {
    pub title: String,
    pub body: String,
    /// What to do in a terminal, when the body does not already say it.
    pub advice: String,
}

/// pacman lists a package that replaces its own name in every update, so "run it once" would not be true.
fn self_replacement_words(name: &str) -> BlockWords {
    BlockWords {
        title: format!(
            "{name} marks itself as a replacement, so Tidewise can't update while it's installed"
        ),
        body: String::new(),
        advice: "Update with sudo pacman -Syu in a terminal instead.".to_string(),
    }
}

pub fn block_words(block: &UpdateBlock) -> BlockWords {
    if let UpdateBlock::Replacement(replacements) = block
        && let [only] = replacements.as_slice()
        && only.new.name == only.old
    {
        return self_replacement_words(&only.old);
    }
    let (title, body, advice) = match block {
        UpdateBlock::Replacement(_) => (
            "This update includes a replacement Tidewise can't do yet",
            String::new(),
            "Run sudo pacman -Syu once in a terminal; pacman will ask you about it.",
        ),
        UpdateBlock::Held(HeldBlock { package, held }) => (
            "An update needs a package you've held back",
            format!(
                "{package} needs a newer {held}, which is held by your pacman.conf. Nothing can be updated here \
                 until you remove it from IgnorePkg, or run sudo pacman -Syu in a terminal and choose to skip \
                 {package}."
            ),
            "",
        ),
        UpdateBlock::Conflict(conflicts) => {
            const STOPPED: &str = "Tidewise never removes packages during an update, so it stopped here. \
                                   Nothing was changed.";
            let body = match conflicts.as_slice() {
                [one] => format!(
                    "The new {} can't be installed next to your {}. {STOPPED}",
                    one.package, one.installed
                ),
                several => {
                    let list: String = several
                        .iter()
                        .map(|c| format!("\n{} (conflicts with {})", c.package, c.installed))
                        .collect();
                    format!(
                        "Some new packages can't be installed next to ones you have. {STOPPED}{list}"
                    )
                }
            };
            (
                "This update would need to remove a package",
                body,
                "Run sudo pacman -Syu in a terminal; pacman will ask before removing anything.",
            )
        }
        UpdateBlock::Pick(picks) => {
            // One name is enough to explain it; pacman asks about every one in the terminal.
            let first = picks.first().map_or("something", |p| p.dependency.as_str());
            let more = match picks.len().saturating_sub(1) {
                0 => String::new(),
                count => format!(" and {count} more"),
            };
            (
                "This update needs a choice Tidewise can't make yet",
                format!(
                    "One of the updates now needs {first}{more}, and several packages can provide it. Tidewise \
                     doesn't ask that question during updates yet, so it won't let pacman pick for you. Nothing was \
                     changed."
                ),
                "Run sudo pacman -Syu in a terminal; pacman will ask you which one.",
            )
        }
    };
    BlockWords {
        title: title.to_string(),
        body,
        advice: advice.to_string(),
    }
}

/// The body of a failure card after the helper refused an update. `plain_text` is written for installs where it says
/// "Go back and choose again", which an update has no way to do.
pub fn refusal_body(kind: &ErrorKind) -> &'static str {
    match kind {
        ErrorKind::ProviderNotChosen => {
            "pacman would have chosen a package by itself, so nothing was changed. Run sudo pacman -Syu in a \
             terminal; pacman will ask you which one."
        }
        other => other.plain_text(),
    }
}

pub fn held_line(held: &PendingUpdate) -> String {
    format!(
        "{} {} → {}, not updated.",
        held.name, held.installed_version, held.new_version
    )
}

/// Whether installing these names calls for a restart: a kernel, systemd or a graphics driver.
pub fn needs_restart<'a>(installed: impl IntoIterator<Item = &'a str>) -> bool {
    installed.into_iter().any(|name| {
        KERNELS.contains(&name) || matches!(name, "systemd" | "mesa") || name.starts_with("nvidia")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_pacman::PackageRef;

    fn pkg(name: &str) -> PackageRef {
        PackageRef {
            repo: "extra".to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn the_fixed_lines_are_exactly_these() {
        assert_eq!(NOTHING_REMOVED, "Nothing will be removed.");
        assert_eq!(
            NEWS_LINE,
            "Before updating, check Arch news for steps you must do by hand: archlinux.org/news"
        );
        assert_eq!(UP_TO_DATE, "Your system is up to date");
        assert_eq!(
            LISTS_CHANGED,
            "The package lists changed since you looked. Tidewise got the new list."
        );
        assert_eq!(
            RESTART_LINE,
            "A new kernel or core system part was installed. Restart when convenient."
        );
    }

    #[test]
    fn the_held_box_is_titled_and_each_row_says_not_updated() {
        assert_eq!(HELD_TITLE, "Held back by your pacman.conf");
        assert_eq!(
            HELD_WARNING,
            "A held package can stop other updates from working."
        );
        let line = held_line(&PendingUpdate {
            name: "linux".to_string(),
            installed_version: "1-1".to_string(),
            new_version: "2-1".to_string(),
            ignored: true,
        });
        assert_eq!(line, "linux 1-1 → 2-1, not updated.");
    }

    #[test]
    fn a_held_block_names_both_packages_and_the_way_out() {
        let words = block_words(&UpdateBlock::Held(HeldBlock {
            package: "needs-upd".to_string(),
            held: "upd".to_string(),
        }));
        assert_eq!(words.title, "An update needs a package you've held back");
        assert_eq!(
            words.body,
            "needs-upd needs a newer upd, which is held by your pacman.conf. Nothing can be updated here until you \
             remove it from IgnorePkg, or run sudo pacman -Syu in a terminal and choose to skip needs-upd."
        );
        assert_eq!(words.advice, "");
    }

    #[test]
    fn a_replacement_sends_the_user_to_the_terminal_once() {
        let words = block_words(&UpdateBlock::Replacement(vec![Replacement {
            new: pkg("new"),
            old: "old".to_string(),
        }]));
        assert_eq!(
            words.title,
            "This update includes a replacement Tidewise can't do yet"
        );
        assert_eq!(words.body, "");
        assert_eq!(
            words.advice,
            "Run sudo pacman -Syu once in a terminal; pacman will ask you about it."
        );
    }

    fn replacement(new: &str, old: &str) -> Replacement {
        Replacement {
            new: pkg(new),
            old: old.to_string(),
        }
    }

    /// Running `-Syu` once does not clear this one: pacman lists it in every update.
    #[test]
    fn a_package_that_only_replaces_itself_is_not_cured_by_running_pacman_once() {
        let words = block_words(&UpdateBlock::Replacement(vec![replacement(
            "python-pylint-pydantic",
            "python-pylint-pydantic",
        )]));
        assert_eq!(
            words.title,
            "python-pylint-pydantic marks itself as a replacement, so Tidewise can't update while it's installed"
        );
        assert_eq!(words.body, "");
        assert_eq!(
            words.advice,
            "Update with sudo pacman -Syu in a terminal instead."
        );
    }

    /// The self-replacing one is what keeps Tidewise blocked, so it is named even when it is listed second.
    #[test]
    fn a_self_replacement_next_to_a_normal_one_names_itself_and_lists_the_other() {
        let words = block_words(&UpdateBlock::Replacement(vec![
            replacement("new", "old"),
            replacement("python-pylint-pydantic", "python-pylint-pydantic"),
        ]));
        assert_eq!(
            words.title,
            "python-pylint-pydantic marks itself as a replacement, so Tidewise can't update while it's installed"
        );
        assert_eq!(words.body, "Also replaced: old.");
        assert_eq!(
            words.advice,
            "Update with sudo pacman -Syu in a terminal instead."
        );
    }

    #[test]
    fn two_self_replacements_name_the_first_and_list_the_second() {
        let words = block_words(&UpdateBlock::Replacement(vec![
            replacement("python-pylint-plugin-utils", "python-pylint-plugin-utils"),
            replacement("python-pylint-pydantic", "python-pylint-pydantic"),
        ]));
        assert_eq!(
            words.title,
            "python-pylint-plugin-utils marks itself as a replacement, so Tidewise can't update while it's installed"
        );
        assert_eq!(words.body, "Also replaced: python-pylint-pydantic.");
        assert_eq!(
            words.advice,
            "Update with sudo pacman -Syu in a terminal instead."
        );
    }

    /// Nothing listed is hidden, and a name listed twice is said once.
    #[test]
    fn every_other_replaced_name_is_listed_once() {
        let words = block_words(&UpdateBlock::Replacement(vec![
            replacement("self", "self"),
            replacement("self", "self-git"),
            replacement("new", "old"),
            replacement("newer", "old"),
        ]));
        assert_eq!(words.body, "Also replaced: self-git, old.");
    }

    #[test]
    fn a_conflict_says_a_package_would_have_to_go() {
        let words = block_words(&UpdateBlock::Conflict(vec![Conflict {
            package: "a".to_string(),
            installed: "b".to_string(),
        }]));
        assert_eq!(words.title, "This update would need to remove a package");
        assert_eq!(
            words.body,
            "The new a can't be installed next to your b. Tidewise never removes packages during an update, so it \
             stopped here. Nothing was changed."
        );
        assert_eq!(
            words.advice,
            "Run sudo pacman -Syu in a terminal; pacman will ask before removing anything."
        );
    }

    #[test]
    fn several_conflicts_are_one_sentence_and_a_list() {
        let conflict = |package: &str, installed: &str| Conflict {
            package: package.to_string(),
            installed: installed.to_string(),
        };
        let words = block_words(&UpdateBlock::Conflict(vec![
            conflict("a", "b"),
            conflict("c", "d"),
        ]));
        assert_eq!(words.title, "This update would need to remove a package");
        assert_eq!(
            words.body,
            "Some new packages can't be installed next to ones you have. Tidewise never removes packages during \
             an update, so it stopped here. Nothing was changed.\na (conflicts with b)\nc (conflicts with d)"
        );
    }

    #[test]
    fn several_picks_name_the_first_and_count_the_rest() {
        let pick = |dependency: &str| ProviderPick {
            dependency: dependency.to_string(),
            provider: pkg("prov-a"),
            version: None,
        };
        for (picks, start) in [
            (
                vec![pick("virtual"), pick("other")],
                "One of the updates now needs virtual and 1 more, and several packages can provide it.",
            ),
            (
                vec![pick("virtual"), pick("other"), pick("third")],
                "One of the updates now needs virtual and 2 more, and several packages can provide it.",
            ),
        ] {
            let body = block_words(&UpdateBlock::Pick(picks)).body;
            assert!(body.starts_with(start), "{body}");
        }
    }

    #[test]
    fn a_pick_says_tidewise_cannot_ask_yet() {
        let words = block_words(&UpdateBlock::Pick(vec![ProviderPick {
            dependency: "virtual".to_string(),
            provider: pkg("prov-a"),
            version: None,
        }]));
        assert_eq!(
            words.title,
            "This update needs a choice Tidewise can't make yet"
        );
        assert_eq!(
            words.body,
            "One of the updates now needs virtual, and several packages can provide it. Tidewise doesn't ask that \
             question during updates yet, so it won't let pacman pick for you. Nothing was changed."
        );
        assert_eq!(
            words.advice,
            "Run sudo pacman -Syu in a terminal; pacman will ask you which one."
        );
    }

    #[test]
    fn a_refused_pick_sends_the_user_to_the_terminal_and_the_rest_keeps_the_helpers_words() {
        assert_eq!(
            refusal_body(&ErrorKind::ProviderNotChosen),
            "pacman would have chosen a package by itself, so nothing was changed. Run sudo pacman -Syu in a \
             terminal; pacman will ask you which one."
        );
        assert_eq!(
            refusal_body(&ErrorKind::DatabaseLocked),
            ErrorKind::DatabaseLocked.plain_text()
        );
    }

    #[test]
    fn a_kernel_systemd_or_a_graphics_driver_calls_for_a_restart() {
        for name in [
            "linux",
            "linux-lts",
            "linux-zen",
            "linux-hardened",
            "linux-rt",
            "linux-rt-lts",
            "systemd",
            "mesa",
            "nvidia",
            "nvidia-utils",
            "nvidia-open-dkms",
        ] {
            assert!(needs_restart(["zsh", name]), "{name}");
        }
    }

    #[test]
    fn an_update_with_none_of_them_calls_for_no_restart() {
        assert!(!needs_restart([
            "zsh",
            "linux-firmware",
            "linux-headers",
            "linux-api-headers",
            "systemd-libs",
            "lib32-mesa",
            "mesa-utils",
        ]));
        assert!(!needs_restart([]));
    }
}
