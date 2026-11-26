//! What the "Update system" cards say (WBS 6.5). Plain strings, no Slint.

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

pub const NOTHING_REMOVED: &str = "";
pub const NEWS_LINE: &str = "";
pub const UP_TO_DATE: &str = "";
pub const LISTS_CHANGED: &str = "";
pub const RESTART_LINE: &str = "";

/// A card's title and body.
pub fn block_words(_block: &UpdateBlock) -> (String, String) {
    (String::new(), String::new())
}

pub fn replacement_line(_replacement: &Replacement) -> String {
    String::new()
}

pub fn held_line(_held: &PendingUpdate) -> String {
    String::new()
}

/// The line under the button when the password window will count more packages than the button does.
pub fn password_count_line(_plan: &UpdatePlan) -> Option<String> {
    None
}

/// Whether installing these names calls for a restart.
pub fn needs_restart<'a>(_installed: impl IntoIterator<Item = &'a str>) -> bool {
    false
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
