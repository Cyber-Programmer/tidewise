//! Parser for `pacman -Qu`: the packages that have a newer version in the sync databases.

use crate::ParseError;

/// One pending update: an installed package whose repository has a newer version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingUpdate {
    pub name: String,
    /// The installed version, as pacman prints it (epoch and release included: `1:1.6.9-1`).
    pub installed_version: String,
    /// The version in the sync database.
    pub new_version: String,
    /// True when pacman marked the line `[ignored]` (the package is in `IgnorePkg` or `IgnoreGroup`).
    /// `pacman -Syu` skips these.
    pub ignored: bool,
}

/// Parses the output of `LC_ALL=C pacman -Qu` into pending updates, in pacman's order.
///
/// Every line must be `name old -> new`, optionally followed by ` [ignored]`. The first bad line
/// (counted from 1) is an error. Empty text means no updates.
pub fn parse_qu(_text: &str) -> Result<Vec<PendingUpdate>, ParseError> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../tests/fixtures/pacman/qu-sample.txt");
    const IGNORED: &str = include_str!("../../../tests/fixtures/pacman/qu-ignored.txt");
    const TRUNCATED: &str = include_str!("../../../tests/fixtures/pacman/qu-broken-truncated.txt");
    const BAD_BYTE: &[u8] = include_bytes!("../../../tests/fixtures/pacman/qu-bad-byte.bin");

    fn update(name: &str, old: &str, new: &str, ignored: bool) -> PendingUpdate {
        PendingUpdate {
            name: name.to_string(),
            installed_version: old.to_string(),
            new_version: new.to_string(),
            ignored,
        }
    }

    fn bad_line(text: &str) -> (usize, String) {
        match parse_qu(text) {
            Err(ParseError::BadUpdateLine { line_number, line }) => (line_number, line),
            other => panic!("expected BadUpdateLine, got {other:?}"),
        }
    }

    #[test]
    fn qu_sample_gives_eight_updates_in_pacmans_order() {
        let updates = parse_qu(SAMPLE).unwrap();
        let names: Vec<&str> = updates.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "7zip",
                "alsa-card-profiles",
                "aom",
                "avahi",
                "bluez",
                "electron43",
                "python-sentry_sdk",
                "wpa_supplicant"
            ]
        );
        assert!(updates.iter().all(|u| !u.ignored));
    }

    #[test]
    fn qu_keeps_epochs_and_release_numbers_in_the_versions() {
        let updates = parse_qu(SAMPLE).unwrap();
        assert_eq!(updates[0], update("7zip", "26.04-1", "26.06-1", false));
        assert_eq!(
            updates[1],
            update("alsa-card-profiles", "1:1.6.9-1", "1:1.6.11-1", false)
        );
        assert_eq!(
            updates[3],
            update("avahi", "1:0.9rc5-1", "1:0.9rc5-2", false)
        );
        assert_eq!(
            updates[7],
            update("wpa_supplicant", "2:2.12-1", "2:2.12-2", false)
        );
    }

    #[test]
    fn qu_ignored_marker_sets_the_flag_and_nothing_else_changes() {
        let updates = parse_qu(IGNORED).unwrap();
        assert_eq!(updates.len(), 8);
        let flags: Vec<(&str, bool)> = updates
            .iter()
            .map(|u| (u.name.as_str(), u.ignored))
            .collect();
        assert_eq!(
            flags,
            [
                ("7zip", true),
                ("alsa-card-profiles", false),
                ("aom", true),
                ("avahi", false),
                ("bluez", false),
                ("electron43", false),
                ("python-sentry_sdk", false),
                ("wpa_supplicant", false),
            ]
        );
        // The marker is not part of the new version.
        assert_eq!(updates[0], update("7zip", "26.04-1", "26.06-1", true));
    }

    #[test]
    fn qu_empty_input_is_no_updates() {
        assert_eq!(parse_qu("").unwrap(), Vec::new());
    }

    #[test]
    fn qu_last_line_may_lack_a_final_newline() {
        let updates = parse_qu("zsh 5.9-1 -> 5.9.1-1").unwrap();
        assert_eq!(updates, [update("zsh", "5.9-1", "5.9.1-1", false)]);
    }

    #[test]
    fn qu_rejects_a_blank_line() {
        assert_eq!(bad_line("zsh 1-1 -> 2-1\n\nbc 1-1 -> 2-1\n").0, 2);
    }

    #[test]
    fn qu_rejects_missing_or_extra_words() {
        assert_eq!(bad_line("zsh 1-1 ->\n"), (1, "zsh 1-1 ->".to_string()));
        assert_eq!(bad_line("zsh 1-1 2-1\n").0, 1);
        assert_eq!(bad_line("zsh 1-1 -> 2-1 extra\n").0, 1);
        assert_eq!(bad_line("zsh\n").0, 1);
    }

    #[test]
    fn qu_rejects_a_wrong_arrow() {
        assert_eq!(bad_line("ok 1-1 -> 2-1\nzsh 1-1 => 2-1\n").0, 2);
        assert_eq!(bad_line("zsh 1-1 - 2-1\n").0, 1);
    }

    #[test]
    fn qu_rejects_extra_spaces_and_tabs() {
        assert_eq!(bad_line("zsh  1-1 -> 2-1\n").0, 1);
        assert_eq!(bad_line(" zsh 1-1 -> 2-1\n").0, 1);
        assert_eq!(bad_line("zsh 1-1 -> 2-1 \n").0, 1);
        assert_eq!(bad_line("zsh\t1-1 -> 2-1\n").0, 1);
    }

    #[test]
    fn qu_rejects_a_bad_package_name() {
        assert_eq!(
            bad_line("zsh 1-1 -> 2-1\n--force 1-1 -> 2-1\n"),
            (2, "--force 1-1 -> 2-1".to_string())
        );
        assert_eq!(bad_line("foo/bar 1-1 -> 2-1\n").0, 1);
        assert_eq!(bad_line("\u{fffd}zsh 1-1 -> 2-1\n").0, 1);
    }

    #[test]
    fn qu_rejects_the_ignored_marker_anywhere_but_the_end() {
        assert_eq!(bad_line("zsh [ignored] 1-1 -> 2-1\n").0, 1);
        assert_eq!(bad_line("zsh 1-1 -> [ignored]\n").0, 1);
        assert_eq!(bad_line("zsh 1-1 -> 2-1 [ignore]\n").0, 1);
        assert_eq!(bad_line("zsh 1-1 -> 2-1 [ignored] [ignored]\n").0, 1);
    }

    #[test]
    fn qu_rejects_control_characters_in_versions() {
        assert_eq!(bad_line("zsh 1\u{7}-1 -> 2-1\n").0, 1);
        assert_eq!(bad_line("zsh 1-1 -> 2\r-1\n").0, 1);
    }

    #[test]
    fn qu_truncated_text_is_an_error_not_a_panic() {
        let (line_number, line) = bad_line(TRUNCATED);
        assert_eq!(line_number, 4);
        assert_eq!(line, "avahi 1:0.9rc5-1 -");
    }

    #[test]
    fn qu_bad_byte_in_a_name_is_refused() {
        // run.rs decodes lossily, so the 0xFF becomes U+FFFD, which is not a legal name character.
        let text = String::from_utf8_lossy(BAD_BYTE);
        assert_eq!(bad_line(&text).0, 5);
    }
}
