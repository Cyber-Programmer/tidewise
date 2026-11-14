//! Step 10: one journal entry per request that got past the start checks (threat T-14, open question 7).
//!
//! No new dependency: journald's native protocol is one datagram to `/run/systemd/journal/socket`, made of
//! `KEY=value\n` lines (<https://systemd.io/JOURNAL_NATIVE_PROTOCOL/>). That document says a value that contains a
//! new line must use a second, binary encoding; we avoid it by making every value printable ASCII (anything else
//! becomes `?`), so a caller cannot add a line or a field. Keys that start with `_` are ignored by journald when a
//! client sends them, and we use none. The standard library's `UnixDatagram` is all we need.
//!
//! What goes in: who asked (`PKEXEC_UID`, as a number or `unknown`), the outcome code, and the **recomputed**
//! summary of a request that passed validation. What never goes in: the raw request, the argv text, any name a
//! caller chose that did not pass validation. A failed send changes neither the outcome nor the exit code.

use std::io;
use std::path::Path;

use tidewise_helper_protocol::ErrorKind;

use crate::gate::Verdict;

/// journald's native socket (`/run/systemd/journal/socket`).
pub const JOURNAL_SOCKET: &str = "/run/systemd/journal/socket";

/// The fixed code of an outcome, as it appears in the log (the same snake_case words as the wire format).
pub fn outcome_code(error: &ErrorKind) -> &'static str {
    let _ = error;
    ""
}

/// The datagram for one request: `KEY=value\n` lines, all values printable ASCII.
pub fn record(caller: Option<u32>, verdict: &Verdict) -> Vec<u8> {
    let _ = (caller, verdict);
    Vec::new()
}

/// Sends one datagram to `socket`. Gives up after two seconds rather than wait for a stuck journald.
pub fn send(socket: &Path, datagram: &[u8]) -> io::Result<()> {
    let _ = (socket, datagram);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixDatagram;
    use std::path::PathBuf;
    use std::time::Duration;
    use tidewise_helper_protocol::{Details, Field, InvalidRequest, Reason};

    fn verdict(error: ErrorKind, summary: Option<&str>) -> Verdict {
        Verdict {
            error,
            summary: summary.map(str::to_string),
        }
    }

    fn text(datagram: &[u8]) -> String {
        String::from_utf8(datagram.to_vec()).unwrap()
    }

    fn all_error_kinds() -> Vec<ErrorKind> {
        vec![
            ErrorKind::TooLarge,
            ErrorKind::Malformed,
            ErrorKind::VersionMismatch,
            ErrorKind::Invalid(InvalidRequest {
                field: Field::Targets,
                index: 0,
                reason: Reason::BadPackageName,
            }),
            ErrorKind::SummaryMismatch,
            ErrorKind::TransactionChanged,
            ErrorKind::SystemOutOfDate,
            ErrorKind::DatabaseLocked,
            ErrorKind::PacmanFailed(Details::from_raw("error: x")),
            ErrorKind::NotImplemented,
        ]
    }

    // ---- the datagram -----------------------------------------------------------------------------------

    #[test]
    fn the_exact_text_for_a_valid_request_that_is_not_implemented_yet() {
        let datagram = record(
            Some(1000),
            &verdict(
                ErrorKind::NotImplemented,
                Some("update system (3 packages)"),
            ),
        );
        assert_eq!(
            text(&datagram),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=6\n\
             TIDEWISE_CALLER_UID=1000\n\
             TIDEWISE_OUTCOME=not_implemented\n\
             TIDEWISE_SUMMARY=update system (3 packages)\n\
             MESSAGE=uid 1000: not_implemented: update system (3 packages)\n"
        );
    }

    #[test]
    fn a_refusal_is_a_warning_and_an_unknown_caller_says_unknown() {
        let datagram = record(
            None,
            &verdict(
                ErrorKind::SummaryMismatch,
                Some("update system (1 package)"),
            ),
        );
        assert_eq!(
            text(&datagram),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=4\n\
             TIDEWISE_CALLER_UID=unknown\n\
             TIDEWISE_OUTCOME=summary_mismatch\n\
             TIDEWISE_SUMMARY=update system (1 package)\n\
             MESSAGE=uid unknown: summary_mismatch: update system (1 package)\n"
        );
    }

    #[test]
    fn a_request_that_did_not_pass_validation_has_no_summary_line() {
        let datagram = record(Some(0), &verdict(ErrorKind::Malformed, None));
        assert_eq!(
            text(&datagram),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=4\n\
             TIDEWISE_CALLER_UID=0\n\
             TIDEWISE_OUTCOME=malformed\n\
             MESSAGE=uid 0: malformed\n"
        );
    }

    #[test]
    fn every_line_is_a_plain_key_equals_value_and_nothing_hides_in_a_value() {
        let hostile = "a\nMESSAGE=evil\0\u{1b}[31m caf\u{e9}\r\nPRIORITY=0\u{202e}";
        for kind in all_error_kinds() {
            let datagram = record(Some(1000), &verdict(kind, Some(hostile)));
            let text = text(&datagram);
            assert!(text.ends_with('\n'));
            let lines: Vec<&str> = text.strip_suffix('\n').unwrap().split('\n').collect();
            // identifier, priority, caller, outcome, summary, message: always six lines, whatever the value held.
            assert_eq!(lines.len(), 6, "{text:?}");
            for line in &lines {
                let (key, value) = line.split_once('=').unwrap();
                assert!(
                    !key.is_empty()
                        && key.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
                        && !key.starts_with('_'),
                    "{line:?}"
                );
                assert!(
                    value.bytes().all(|b| (0x20..=0x7e).contains(&b)),
                    "{line:?}"
                );
            }
            assert_eq!(text.matches("MESSAGE=").count(), 1, "{text:?}");
            assert_eq!(text.matches("PRIORITY=").count(), 1, "{text:?}");
        }
    }

    #[test]
    fn a_non_ascii_character_becomes_one_question_mark() {
        let datagram = record(
            Some(1),
            &verdict(ErrorKind::SummaryMismatch, Some("caf\u{e9}")),
        );
        assert!(text(&datagram).contains("TIDEWISE_SUMMARY=caf?\n"));
    }

    #[test]
    fn only_a_not_implemented_outcome_is_info_everything_else_warns() {
        for kind in all_error_kinds() {
            let expected = if kind == ErrorKind::NotImplemented {
                "PRIORITY=6\n"
            } else {
                "PRIORITY=4\n"
            };
            let datagram = record(Some(1), &verdict(kind.clone(), None));
            assert!(text(&datagram).contains(expected), "{kind:?}");
        }
    }

    #[test]
    fn every_outcome_has_its_own_snake_case_code() {
        let codes: Vec<&str> = all_error_kinds().iter().map(outcome_code).collect();
        for code in &codes {
            assert!(
                !code.is_empty()
                    && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                    && !code.starts_with('_')
                    && !code.ends_with('_'),
                "{code:?}"
            );
        }
        let mut unique = codes.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), codes.len(), "{codes:?}");
        assert_eq!(outcome_code(&ErrorKind::NotImplemented), "not_implemented");
        assert_eq!(
            outcome_code(&ErrorKind::SummaryMismatch),
            "summary_mismatch"
        );
    }

    #[test]
    fn the_raw_request_and_the_argv_text_never_reach_the_entry() {
        let request = r#"{"protocol_version":1,"action":{"update_system":{"transaction":[{"repo":"core","name":"SECRET-PATH/etc/shadow","version":"1-1"}]}}}"#;
        let verdict = crate::gate::decide(b"SECRET-ARGV", Ok(request.as_bytes().to_vec()));
        let datagram = text(&record(Some(1000), &verdict));
        assert!(!datagram.contains("SECRET"), "{datagram}");
        assert!(
            datagram.contains("TIDEWISE_OUTCOME=invalid\n"),
            "{datagram}"
        );
    }

    // ---- sending ----------------------------------------------------------------------------------------

    /// A folder for a test socket, removed when dropped. (No `tempfile` crate: this crate has one dependency.)
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(test: &str) -> TempDir {
            let path = std::env::temp_dir().join(format!(
                "tidewise-helper-journal-{}-{test}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_datagram_arrives_whole_on_a_socket_in_a_temp_folder() {
        let dir = TempDir::new("arrives");
        let path = dir.0.join("socket");
        let listener = UnixDatagram::bind(&path).unwrap();
        listener
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let datagram = record(
            Some(1000),
            &verdict(
                ErrorKind::NotImplemented,
                Some("update system (3 packages)"),
            ),
        );
        send(&path, &datagram).unwrap();

        let mut buffer = vec![0u8; 4096];
        let n = listener.recv(&mut buffer).unwrap();
        assert_eq!(&buffer[..n], datagram.as_slice());
    }

    #[test]
    fn sending_to_a_missing_socket_is_an_error_not_a_crash() {
        let dir = TempDir::new("missing");
        assert!(send(&dir.0.join("nope"), b"x=y\n").is_err());
    }

    #[test]
    fn sending_to_something_that_is_not_a_socket_is_an_error() {
        let dir = TempDir::new("not-a-socket");
        let path = dir.0.join("file");
        std::fs::write(&path, b"").unwrap();
        assert!(send(&path, b"x=y\n").is_err());
    }
}
