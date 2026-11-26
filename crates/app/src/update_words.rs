//! What the "Update system" cards say (WBS 6.5). Plain strings, no Slint.

use tidewise_helper_protocol::ErrorKind;
use tidewise_pacman::{Conflict, HeldBlock, PendingUpdate, ProviderPick, Replacement, UpdatePlan};

/// Why the review offers no Update button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateBlock {
    /// A held package would have to be upgraded too: pacman upgrades nothing.
    Held(HeldBlock),
    /// The real run would abort.
    Conflict(Vec<Conflict>),
    /// pacman would choose a package by itself; the helper would refuse (golden rule 10).
    Pick(Vec<ProviderPick>),
}

pub const NOTHING_REMOVED: &str = "Nothing will be removed.";
/// Rule 13, partly: the address as text, the user reads it (no network of our own, rule 11).
pub const NEWS_LINE: &str =
    "Before updating, check Arch news for steps you must do by hand: archlinux.org/news";
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

/// A card's title and body.
pub fn block_words(block: &UpdateBlock) -> (String, String) {
    let (title, body) = match block {
        UpdateBlock::Held(HeldBlock { package, held }) => (
            "A held package blocks this update",
            format!(
                "{package} needs a newer {held}, which is held by your pacman.conf. Nothing can be updated here \
                 until you remove it from IgnorePkg, or run sudo pacman -Syu in a terminal and choose to skip \
                 {package}."
            ),
        ),
        UpdateBlock::Conflict(conflicts) => {
            let pairs: Vec<String> = conflicts
                .iter()
                .map(|c| format!("{} conflicts with {}.", c.package, c.installed))
                .collect();
            (
                "This update conflicts with an installed package",
                format!(
                    "{} Tidewise doesn't remove packages; run sudo pacman -Syu in a terminal to decide.",
                    pairs.join(" ")
                ),
            )
        }
        UpdateBlock::Pick(picks) => {
            let pairs: Vec<String> = picks
                .iter()
                .map(|p| format!("{} for {}", p.provider.name, p.dependency))
                .collect();
            (
                "This update needs a choice",
                format!(
                    "pacman would add {} without asking you. Tidewise can't ask that during a system update yet; \
                     run sudo pacman -Syu in a terminal to choose.",
                    pairs.join(", ")
                ),
            )
        }
    };
    (title.to_string(), body)
}

/// The body of a failure card after the helper refused an update. `plain_text` is written for installs where it says
/// "Go back and choose again", which an update has no way to do.
pub fn refusal_body(_kind: &ErrorKind) -> &'static str {
    ""
}

pub fn replacement_line(replacement: &Replacement) -> String {
    format!(
        "{} replaces {}. Tidewise doesn't do replacements yet; run sudo pacman -Syu in a terminal to accept it.",
        replacement.new.name, replacement.old
    )
}

pub fn held_line(held: &PendingUpdate) -> String {
    format!(
        "{} {} → {} is held by your pacman.conf.",
        held.name, held.installed_version, held.new_version
    )
}

/// The line under the button when the password window will count more packages than the button does: the helper is
/// sent, and counts, the whole list.
pub fn password_count_line(plan: &UpdatePlan) -> Option<String> {
    (!plan.skipped.is_empty()).then(|| {
        format!(
            "The password window will count {} packages: it includes what Tidewise skips.",
            plan.items.len()
        )
    })
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
    use tidewise_pacman::{PackageRef, PlanItem, Role};

    fn pkg(name: &str) -> PackageRef {
        PackageRef {
            repo: "extra".to_string(),
            name: name.to_string(),
        }
    }

    fn item(name: &str) -> PlanItem {
        PlanItem {
            package: pkg(name),
            version: "2-1".to_string(),
            role: Role::Update,
            installed_version: Some("1-1".to_string()),
            download_size: 1,
            installed_size: 2,
            installed_size_text: String::new(),
            current_size: Some(1),
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
    fn a_replacement_says_what_it_means_and_how_to_accept_it() {
        let line = replacement_line(&Replacement {
            new: pkg("new-pkg"),
            old: "old-pkg".to_string(),
        });
        assert_eq!(
            line,
            "new-pkg replaces old-pkg. Tidewise doesn't do replacements yet; run sudo pacman -Syu in a terminal to \
             accept it."
        );
    }

    #[test]
    fn a_held_package_says_it_is_held_by_pacman_conf() {
        let line = held_line(&PendingUpdate {
            name: "linux".to_string(),
            installed_version: "1-1".to_string(),
            new_version: "2-1".to_string(),
            ignored: true,
        });
        assert_eq!(line, "linux 1-1 → 2-1 is held by your pacman.conf.");
    }

    #[test]
    fn a_held_block_names_both_packages_and_the_way_out() {
        let (title, body) = block_words(&UpdateBlock::Held(HeldBlock {
            package: "needs-upd".to_string(),
            held: "upd".to_string(),
        }));
        assert_eq!(title, "A held package blocks this update");
        assert_eq!(
            body,
            "needs-upd needs a newer upd, which is held by your pacman.conf. Nothing can be updated here until you \
             remove it from IgnorePkg, or run sudo pacman -Syu in a terminal and choose to skip needs-upd."
        );
    }

    #[test]
    fn a_conflict_and_a_pick_send_the_user_to_the_terminal() {
        let (title, body) = block_words(&UpdateBlock::Conflict(vec![Conflict {
            package: "a".to_string(),
            installed: "b".to_string(),
        }]));
        assert_eq!(title, "This update conflicts with an installed package");
        assert_eq!(
            body,
            "a conflicts with b. Tidewise doesn't remove packages; run sudo pacman -Syu in a terminal to decide."
        );
        let (title, body) = block_words(&UpdateBlock::Pick(vec![ProviderPick {
            dependency: "virtual".to_string(),
            provider: pkg("prov-a"),
        }]));
        assert_eq!(title, "This update needs a choice");
        assert_eq!(
            body,
            "pacman would add prov-a for virtual without asking you. Tidewise can't ask that during a system update \
             yet; run sudo pacman -Syu in a terminal to choose."
        );
    }

    #[test]
    fn a_refused_pick_sends_the_user_to_the_terminal_and_the_rest_keeps_the_helpers_words() {
        assert_eq!(
            refusal_body(&ErrorKind::ProviderNotChosen),
            "pacman would have chosen a package by itself, so nothing was changed. Run sudo pacman -Syu in a \
             terminal to choose."
        );
        assert_eq!(
            refusal_body(&ErrorKind::DatabaseLocked),
            ErrorKind::DatabaseLocked.plain_text()
        );
    }

    #[test]
    fn the_password_window_line_appears_only_when_something_listed_is_skipped() {
        let mut plan = UpdatePlan {
            items: vec![item("upd")],
            ..UpdatePlan::default()
        };
        assert_eq!(password_count_line(&plan), None);
        plan.items.push(item("new"));
        plan.skipped.push(pkg("new"));
        assert_eq!(
            password_count_line(&plan),
            Some(
                "The password window will count 2 packages: it includes what Tidewise skips."
                    .to_string()
            )
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
