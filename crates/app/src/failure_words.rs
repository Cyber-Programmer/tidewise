//! What a failed or short helper run says, the same on the install and update cards (WBS 6.5, manual check of 27 Nov).
//! Plain strings, no Slint.

use tidewise_helper_protocol::{Details, ErrorKind, Notice};

pub const DOWNLOAD_FAILED: &str = "Tidewise couldn't download the packages. Your connection may be slow or a mirror \
                                   may be down. Nothing was changed.";
pub const UPDATE_RUNNING: &str = "Another update is still running (maybe one Tidewise started earlier). Wait for it to \
                                  finish, then check again.";
pub const NOTHING_INSTALLED: &str = "Nothing was installed.";

/// Whether the helper's count says that not one package of the confirmed list of `listed` was installed.
pub fn nothing_installed(notices: &[Notice], listed: usize) -> bool {
    notices.iter().any(|notice| match notice {
        Notice::PackagesNotInstalled(count) => {
            listed > 0 && usize::try_from(*count).is_ok_and(|count| count >= listed)
        }
        _ => false,
    })
}

/// A notice in words. `listed` is how many packages the confirmed list had.
pub fn notice_line(notice: &Notice, listed: usize) -> String {
    if nothing_installed(std::slice::from_ref(notice), listed) {
        NOTHING_INSTALLED.to_string()
    } else {
        notice.plain_text()
    }
}

/// The card body for a refusal or failure that has no words of its own on a screen.
pub fn refusal_text(kind: &ErrorKind) -> &'static str {
    match kind {
        // The helper's own words blame "another package manager"; the usual holder is an update still running.
        ErrorKind::DatabaseLocked => UPDATE_RUNNING,
        other => other.plain_text(),
    }
}

/// pacman's words, when the run failed because the packages (or lists) could not be downloaded and so nothing was
/// changed.
pub fn download_failure<'a>(
    kind: &'a ErrorKind,
    notices: &[Notice],
    listed: usize,
) -> Option<&'a Details> {
    let ErrorKind::PacmanFailed(details) = kind else {
        return None;
    };
    // "Nothing was changed" needs the helper's counts to agree, when it sent any.
    let something_changed = notices.iter().any(|notice| match notice {
        Notice::PackagesRemoved(_) => true,
        Notice::PackagesNotInstalled(_) => !nothing_installed(std::slice::from_ref(notice), listed),
        Notice::AuditEntryNotWritten => false,
    });
    (!something_changed && says_download_failed(details.as_str())).then_some(details)
}

/// pacman ends a failed run with the reason in brackets. The helper keeps only the first 4 KiB, so when that line was
/// cut off, the `failed retrieving file` lines before it decide.
fn says_download_failed(text: &str) -> bool {
    let closing = text.lines().rfind(|line| {
        line.starts_with("error: failed to commit transaction (")
            || line.starts_with("error: failed to synchronize all databases (")
    });
    match closing {
        Some(line) => {
            line.ends_with("(failed to retrieve some files)")
                || line.ends_with("(download library error)")
        }
        None => text
            .lines()
            .any(|line| line.starts_with("error: failed retrieving file '")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real pacman 7.1 stderr, measured in the test container with an unreachable or stalled mirror.
    const MEASURED: [&str; 6] = [
        include_str!("../../../tests/fixtures/pacman/container/download-su-unresolvable.stderr"),
        include_str!("../../../tests/fixtures/pacman/container/download-su-refused.stderr"),
        include_str!("../../../tests/fixtures/pacman/container/download-su-stalled.stderr"),
        include_str!("../../../tests/fixtures/pacman/container/download-su-404.stderr"),
        include_str!("../../../tests/fixtures/pacman/container/download-sy-unresolvable.stderr"),
        include_str!("../../../tests/fixtures/pacman/container/download-sy-stalled.stderr"),
    ];

    fn failed(text: &str) -> ErrorKind {
        ErrorKind::PacmanFailed(Details::from_raw(text))
    }

    fn is_download(text: &str, notices: &[Notice]) -> bool {
        download_failure(&failed(text), notices, 3).is_some()
    }

    #[test]
    fn the_new_texts_are_exactly_these() {
        assert_eq!(
            DOWNLOAD_FAILED,
            "Tidewise couldn't download the packages. Your connection may be slow or a mirror may be down. Nothing \
             was changed."
        );
        assert_eq!(
            UPDATE_RUNNING,
            "Another update is still running (maybe one Tidewise started earlier). Wait for it to finish, then check \
             again."
        );
        assert_eq!(NOTHING_INSTALLED, "Nothing was installed.");
    }

    #[test]
    fn every_measured_download_failure_is_recognised() {
        for text in MEASURED {
            // A failed list refresh sends no notice; a failed real run says every package is missing.
            assert!(is_download(text, &[]), "{text}");
            assert!(
                is_download(text, &[Notice::PackagesNotInstalled(3)]),
                "{text}"
            );
        }
        assert!(is_download(
            "error: failed to commit transaction (download library error)",
            &[]
        ));
    }

    /// The helper keeps only the first 4 KiB, so the closing line of a long error can be missing.
    #[test]
    fn a_cut_off_error_is_judged_by_its_retrieving_lines() {
        let line =
            "error: failed retrieving file 'a.pkg.tar.zst' from m : Could not resolve host: m\n";
        assert!(is_download(&line.repeat(200), &[]));
    }

    #[test]
    fn another_failure_is_not_called_a_download_failure() {
        // A mirror failed, another one worked, and the install then broke for its own reason.
        let recovered = "error: failed retrieving file 'a' from m : Operation too slow\n\
                         error: failed to commit transaction (conflicting files)";
        for text in [
            recovered,
            "error: failed to commit transaction (conflicting files)",
            "error: x",
            "",
        ] {
            assert!(!is_download(text, &[]), "{text}");
        }
        assert!(download_failure(&ErrorKind::DatabaseLocked, &[], 3).is_none());
    }

    /// "Nothing was changed" is only said when the helper's own count agrees.
    #[test]
    fn a_run_that_changed_something_is_not_called_a_download_failure() {
        let text = MEASURED[0];
        assert!(!is_download(text, &[Notice::PackagesNotInstalled(2)]));
        assert!(!is_download(text, &[Notice::PackagesRemoved(1)]));
    }

    #[test]
    fn nothing_installed_is_said_so() {
        let missing = Notice::PackagesNotInstalled;
        assert_eq!(notice_line(&missing(42), 42), "Nothing was installed.");
        assert_eq!(notice_line(&missing(1), 1), "Nothing was installed.");
        assert_eq!(
            notice_line(&missing(2), 42),
            "2 packages from your list were not installed."
        );
        assert_eq!(
            notice_line(&Notice::PackagesRemoved(1), 1),
            "1 package was removed."
        );
    }

    #[test]
    fn a_held_lock_does_not_blame_another_package_manager() {
        assert_eq!(refusal_text(&ErrorKind::DatabaseLocked), UPDATE_RUNNING);
        assert_eq!(
            refusal_text(&ErrorKind::SystemOutOfDate),
            ErrorKind::SystemOutOfDate.plain_text()
        );
    }
}
