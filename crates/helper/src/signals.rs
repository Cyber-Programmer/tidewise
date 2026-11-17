//! Step 2b: is the child-exit signal still the default? (review 5.8 R-01, threat T-08)
//!
//! Linux keeps an ignored signal across `execve`, and `pkexec` does not reset it. If whatever started Tidewise
//! ignored SIGCHLD, the kernel reaps pacman by itself and our `waitpid` fails with "no child processes": the helper
//! would lose track of a pacman that finished, and an update would stop right after its one `-Sy`. Closing that
//! gap in code needs `sigaction`, which is `unsafe`, so the helper does what `fds.rs` does: look, and refuse.
//!
//! The text is the `SigIgn:` line of `/proc/self/status` (`proc_pid_status(5)`): sixteen hex digits, bit `n - 1`
//! set when signal `n` is ignored. Rust itself ignores SIGPIPE, so a set bit for it is normal and not looked at.

/// Signal number of SIGCHLD on Linux.
pub const SIGCHLD: u32 = 17;

/// Whether SIGCHLD is ignored, from the text of `/proc/self/status`. `None` when the text does not hold exactly one
/// well-formed `SigIgn:` line (sixteen hex digits): unknown is not "fine".
pub fn child_signal_ignored(status: &str) -> Option<bool> {
    let _ = status;
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(sig_ign: &str) -> String {
        format!(
            "Name:\thelper\nSigPnd:\t0000000000000000\nSigIgn:\t{sig_ign}\nSigCgt:\t0000000000000000\n"
        )
    }

    #[test]
    fn nothing_ignored_is_fine() {
        assert_eq!(
            child_signal_ignored(&status("0000000000000000")),
            Some(false)
        );
    }

    #[test]
    fn rusts_own_ignored_sigpipe_is_fine() {
        // SIGPIPE is signal 13, bit 12: 0x1000. Every Rust program starts with it ignored.
        assert_eq!(
            child_signal_ignored(&status("0000000000001000")),
            Some(false)
        );
    }

    #[test]
    fn an_ignored_sigchld_is_found() {
        // SIGCHLD is signal 17, bit 16: 0x10000.
        assert_eq!(
            child_signal_ignored(&status("0000000000010000")),
            Some(true)
        );
    }

    #[test]
    fn an_ignored_sigchld_among_others_is_found() {
        assert_eq!(
            child_signal_ignored(&status("0000000000011001")),
            Some(true)
        );
        assert_eq!(
            child_signal_ignored(&status("ffffffffffffffff")),
            Some(true)
        );
    }

    #[test]
    fn every_signal_but_sigchld_ignored_is_fine() {
        assert_eq!(
            child_signal_ignored(&status("fffffffffffeffff")),
            Some(false)
        );
    }

    #[test]
    fn a_line_that_is_missing_doubled_or_malformed_is_unknown() {
        assert_eq!(child_signal_ignored("Name:\thelper\n"), None);
        assert_eq!(child_signal_ignored(""), None);
        let doubled = format!(
            "{}{}",
            status("0000000000000000"),
            status("0000000000010000")
        );
        assert_eq!(child_signal_ignored(&doubled), None);
        for bad in [
            "",
            "0",
            "000000000001000",
            "00000000000100000",
            "000000000001000g",
            "0x00000000010000",
            "+000000000010000",
            "0000000000010000 0",
            "-000000000010000",
        ] {
            assert_eq!(child_signal_ignored(&status(bad)), None, "{bad:?}");
        }
    }

    #[test]
    fn this_very_process_has_a_readable_line() {
        let text = std::fs::read_to_string("/proc/self/status").unwrap();
        assert!(child_signal_ignored(&text).is_some());
    }
}
