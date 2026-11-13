//! The partial-upgrade rule (golden rule 4) as one pure decision.

use crate::PendingUpdate;

/// Whether installing a new package right now would be a partial upgrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallDecision {
    /// Nothing the system will update is pending, so installing against the current package lists is safe.
    Allowed {
        /// Pending updates held back by `IgnorePkg` / `IgnoreGroup`. `pacman -Syu` skips them too, so they
        /// don't block, but the review screen should say they are held back. The dry run must still catch
        /// a dependency conflict with a held-back package.
        ignored: usize,
    },
    /// The system is behind its package lists: the install must be done together with a full update.
    MustUpdateSystem {
        /// Pending updates `pacman -Syu` would install (not counting ignored ones).
        count: usize,
        /// Pending updates held back by `IgnorePkg` / `IgnoreGroup`.
        ignored: usize,
    },
}

/// Decides from the pending updates (`pending_updates()`) whether an install is allowed.
///
/// Ignored updates don't count towards "must update the system", because `-Syu` would not install them
/// either, so waiting for them could never end. They are counted separately.
pub fn install_decision(updates: &[PendingUpdate]) -> InstallDecision {
    let ignored = updates.iter().filter(|update| update.ignored).count();
    let count = updates.len() - ignored;
    if count == 0 {
        InstallDecision::Allowed { ignored }
    } else {
        InstallDecision::MustUpdateSystem { count, ignored }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_qu;

    const SAMPLE: &str = include_str!("../../../tests/fixtures/pacman/qu-sample.txt");
    const IGNORED: &str = include_str!("../../../tests/fixtures/pacman/qu-ignored.txt");

    fn update(name: &str, ignored: bool) -> PendingUpdate {
        PendingUpdate {
            name: name.to_string(),
            installed_version: "1-1".to_string(),
            new_version: "2-1".to_string(),
            ignored,
        }
    }

    #[test]
    fn nothing_pending_allows_the_install() {
        assert_eq!(
            install_decision(&[]),
            InstallDecision::Allowed { ignored: 0 }
        );
    }

    #[test]
    fn one_pending_update_means_the_system_must_be_updated() {
        assert_eq!(
            install_decision(&[update("zsh", false)]),
            InstallDecision::MustUpdateSystem {
                count: 1,
                ignored: 0
            }
        );
    }

    #[test]
    fn only_ignored_updates_still_allow_the_install_and_are_counted() {
        assert_eq!(
            install_decision(&[update("7zip", true), update("aom", true)]),
            InstallDecision::Allowed { ignored: 2 }
        );
    }

    #[test]
    fn ignored_updates_are_counted_apart_from_the_ones_that_block() {
        let updates = [
            update("7zip", true),
            update("zsh", false),
            update("bc", false),
        ];
        assert_eq!(
            install_decision(&updates),
            InstallDecision::MustUpdateSystem {
                count: 2,
                ignored: 1
            }
        );
    }

    #[test]
    fn the_real_sample_blocks_with_eight_updates() {
        assert_eq!(
            install_decision(&parse_qu(SAMPLE).unwrap()),
            InstallDecision::MustUpdateSystem {
                count: 8,
                ignored: 0
            }
        );
    }

    #[test]
    fn the_real_sample_with_ignore_pkg_blocks_with_six_and_holds_back_two() {
        assert_eq!(
            install_decision(&parse_qu(IGNORED).unwrap()),
            InstallDecision::MustUpdateSystem {
                count: 6,
                ignored: 2
            }
        );
    }
}
