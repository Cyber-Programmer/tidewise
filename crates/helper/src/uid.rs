//! Step 2: are we root? (threat T-01)
//!
//! Safe Rust's standard library has no `geteuid`, so the effective user id is read from the `Uid:` line of
//! `/proc/self/status` (`proc(5)`: four numbers, real, effective, saved set-user-ID, file-system). The parsing is a
//! pure function over the text, so it is tested with fixtures. Anything unexpected counts as "not root".

use std::io;

/// The effective user id in the text of `/proc/self/status`, or `None` if the text is not exactly one well-formed
/// `Uid:` line (four numbers that each fit in a `u32`).
pub fn effective_uid(status: &str) -> Option<u32> {
    let mut uid_line = None;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            if uid_line.is_some() {
                // The kernel prints one. Two means something odd: refuse instead of picking one.
                return None;
            }
            uid_line = Some(rest);
        }
    }

    let ids: Option<Vec<u32>> = uid_line?.split_whitespace().map(parse_id).collect();
    match ids?.as_slice() {
        [_real, effective, _saved, _file_system] => Some(*effective),
        _ => None,
    }
}

/// Digits only (`u32::from_str` would also accept a leading `+`), and the number must fit in a `u32`.
fn parse_id(field: &str) -> Option<u32> {
    if field.bytes().all(|byte| byte.is_ascii_digit()) {
        field.parse().ok()
    } else {
        None
    }
}

/// True only when the effective user id is 0.
pub fn is_root(status: &str) -> bool {
    effective_uid(status) == Some(0)
}

/// Reads `/proc/self/status`.
pub fn read_status() -> io::Result<String> {
    std::fs::read_to_string("/proc/self/status")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(uid_line: &str) -> String {
        format!(
            "Name:\ttidewise-helper\nUmask:\t0022\nState:\tR (running)\nTgid:\t77\n{uid_line}\nGid:\t0\t0\t0\t0\n"
        )
    }

    #[test]
    fn root_is_uid_zero() {
        let text = status("Uid:\t0\t0\t0\t0");
        assert_eq!(effective_uid(&text), Some(0));
        assert!(is_root(&text));
    }

    #[test]
    fn an_ordinary_user_is_not_root() {
        let text = status("Uid:\t1000\t1000\t1000\t1000");
        assert_eq!(effective_uid(&text), Some(1000));
        assert!(!is_root(&text));
    }

    #[test]
    fn the_effective_id_counts_not_the_real_one() {
        // A set-user-ID program: real 1000, effective 0 (this is how pkexec's target looks before setuid(0)).
        let text = status("Uid:\t1000\t0\t0\t0");
        assert_eq!(effective_uid(&text), Some(0));
        // The other way round: real 0 but effective 1000 is not root.
        let text = status("Uid:\t0\t1000\t0\t0");
        assert_eq!(effective_uid(&text), Some(1000));
        assert!(!is_root(&text));
    }

    #[test]
    fn spaces_instead_of_tabs_are_fine() {
        assert_eq!(effective_uid("Uid: 5 6 7 8\n"), Some(6));
    }

    #[test]
    fn a_missing_uid_line_is_not_root() {
        assert_eq!(effective_uid("Name:\tx\nGid:\t0\t0\t0\t0\n"), None);
        assert_eq!(effective_uid(""), None);
        assert!(!is_root(""));
    }

    #[test]
    fn garbage_is_not_root() {
        for line in [
            "Uid:",
            "Uid:\t",
            "Uid:\tx\ty\tz\tw",
            "Uid:\t0\t0\t0",
            "Uid:\t0\t0\t0\t0\t0",
            "Uid:\t0\t-1\t0\t0",
            "Uid:\t0\t+0\t0\t0",
            "Uid:\t0\t0x0\t0\t0",
            "Uid:\t0\t0.0\t0\t0",
            "Uid:\t0\t\u{ff10}\t0\t0",
        ] {
            let text = status(line);
            assert_eq!(effective_uid(&text), None, "{line:?}");
            assert!(!is_root(&text), "{line:?}");
        }
    }

    #[test]
    fn a_number_too_big_for_a_u32_is_not_root() {
        for big in ["4294967296", "99999999999999999999", "18446744073709551616"] {
            let text = status(&format!("Uid:\t0\t{big}\t0\t0"));
            assert_eq!(effective_uid(&text), None, "{big}");
        }
        // 4294967295 is the biggest u32 and is still a (non-root) id.
        assert_eq!(
            effective_uid(&status("Uid:\t0\t4294967295\t0\t0")),
            Some(4294967295)
        );
    }

    #[test]
    fn two_uid_lines_are_refused_not_guessed() {
        let text = "Uid:\t0\t0\t0\t0\nUid:\t1000\t1000\t1000\t1000\n";
        assert_eq!(effective_uid(text), None);
        assert!(!is_root(text));
    }

    #[test]
    fn lines_that_only_end_in_uid_are_not_the_uid_line() {
        // `NSuid:` and friends exist in /proc/<pid>/status; only a line that starts with `Uid:` counts.
        let text = "NSuid:\t0\nXUid:\t0\t0\t0\t0\n";
        assert_eq!(effective_uid(text), None);
    }

    #[test]
    fn leading_zeros_do_not_change_the_number() {
        assert_eq!(effective_uid("Uid:\t0\t000\t0\t0\n"), Some(0));
    }

    #[test]
    fn the_real_status_file_has_a_uid_line_and_it_is_ours_or_not_root() {
        // Tests are not run as root (CI and the human's shell), so only check that the real file parses.
        let text = read_status().unwrap();
        assert!(effective_uid(&text).is_some(), "{text}");
    }
}
