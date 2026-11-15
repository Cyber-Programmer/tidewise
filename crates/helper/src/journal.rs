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
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use std::time::Duration;

use tidewise_helper_protocol::{ErrorKind, Outcome};

/// journald's native socket (`/run/systemd/journal/socket`).
pub const JOURNAL_SOCKET: &str = "/run/systemd/journal/socket";

/// Longest value written to the journal, in characters. Real summaries are at most 200 bytes.
const MAX_VALUE_CHARS: usize = 512;

/// The fixed code of an outcome, as it appears in the log (the same snake_case words as the wire format).
/// No catch-all arm: a new `ErrorKind` will not compile here until it has a code.
pub fn outcome_code(error: &ErrorKind) -> &'static str {
    match error {
        ErrorKind::TooLarge => "too_large",
        ErrorKind::Malformed => "malformed",
        ErrorKind::VersionMismatch => "version_mismatch",
        ErrorKind::Invalid(_) => "invalid",
        ErrorKind::SummaryMismatch => "summary_mismatch",
        ErrorKind::TransactionChanged => "transaction_changed",
        ErrorKind::SystemOutOfDate => "system_out_of_date",
        ErrorKind::DatabaseLocked => "database_locked",
        ErrorKind::PacmanFailed(_) => "pacman_failed",
    }
}

/// Where in its life a request is when an entry is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage<'a> {
    /// The request passed every check. Written before any pacman command, so a helper that is killed later still left
    /// a trace that a transaction began.
    Accepted,
    /// The end: the result the final line will carry (a refused request has only this entry).
    Finished(&'a Outcome),
}

/// The fixed code of a final result: `ok`, or the code of the error.
pub fn finished_code(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Ok => "ok",
        Outcome::Error(error) => outcome_code(error),
    }
}

/// The datagram for one entry: `KEY=value\n` lines, all values printable ASCII. `summary` is only ever the summary the
/// helper recomputed from a request that passed validation.
pub fn record(caller: Option<u32>, stage: Stage, summary: Option<&str>) -> Vec<u8> {
    let uid = caller.map_or_else(|| "unknown".to_string(), |uid| uid.to_string());
    // syslog levels: 6 = info (accepted, or finished fine), 4 = warning (a refusal or a failure).
    let (priority, stage_name, code) = match stage {
        Stage::Accepted => ("6", "accepted", None),
        Stage::Finished(outcome) => {
            let priority = if matches!(outcome, Outcome::Ok) {
                "6"
            } else {
                "4"
            };
            (priority, "finished", Some(finished_code(outcome)))
        }
    };

    let mut text = String::new();
    add_field(&mut text, "SYSLOG_IDENTIFIER", "tidewise-helper");
    add_field(&mut text, "PRIORITY", priority);
    add_field(&mut text, "TIDEWISE_CALLER_UID", &uid);
    add_field(&mut text, "TIDEWISE_STAGE", stage_name);
    if let Some(code) = code {
        add_field(&mut text, "TIDEWISE_OUTCOME", code);
    }
    let label = code.unwrap_or("accepted");
    let message = match summary {
        Some(summary) => {
            add_field(&mut text, "TIDEWISE_SUMMARY", summary);
            format!("uid {uid}: {label}: {summary}")
        }
        None => format!("uid {uid}: {label}"),
    };
    add_field(&mut text, "MESSAGE", &message);
    text.into_bytes()
}

/// Appends `KEY=value\n`. The one place where a value enters the datagram, so the one place that cleans it.
fn add_field(text: &mut String, key: &str, value: &str) {
    text.push_str(key);
    text.push('=');
    text.extend(
        value
            .chars()
            .take(MAX_VALUE_CHARS)
            .map(|c| if (' '..='~').contains(&c) { c } else { '?' }),
    );
    text.push('\n');
}

/// Sends one datagram to `socket`. Gives up after two seconds rather than wait for a stuck journald.
pub fn send(socket: &Path, datagram: &[u8]) -> io::Result<()> {
    let sender = UnixDatagram::unbound()?;
    sender.set_write_timeout(Some(Duration::from_secs(2)))?;
    let sent = sender.send_to(datagram, socket)?;
    if sent == datagram.len() {
        Ok(())
    } else {
        Err(io::Error::other("the journal datagram was cut short"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tidewise_helper_protocol::{Details, Field, InvalidRequest, Reason};

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
        ]
    }

    // ---- the datagram -----------------------------------------------------------------------------------

    fn finished(error: ErrorKind) -> Outcome {
        Outcome::Error(error)
    }

    #[test]
    fn the_exact_text_of_the_accepted_entry() {
        let datagram = record(
            Some(1000),
            Stage::Accepted,
            Some("update system (3 packages)"),
        );
        assert_eq!(
            text(&datagram),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=6\n\
             TIDEWISE_CALLER_UID=1000\n\
             TIDEWISE_STAGE=accepted\n\
             TIDEWISE_SUMMARY=update system (3 packages)\n\
             MESSAGE=uid 1000: accepted: update system (3 packages)\n"
        );
    }

    #[test]
    fn the_exact_text_of_a_successful_finish() {
        let datagram = record(
            Some(1000),
            Stage::Finished(&Outcome::Ok),
            Some("update system (3 packages)"),
        );
        assert_eq!(
            text(&datagram),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=6\n\
             TIDEWISE_CALLER_UID=1000\n\
             TIDEWISE_STAGE=finished\n\
             TIDEWISE_OUTCOME=ok\n\
             TIDEWISE_SUMMARY=update system (3 packages)\n\
             MESSAGE=uid 1000: ok: update system (3 packages)\n"
        );
    }

    #[test]
    fn a_refusal_is_a_warning_and_an_unknown_caller_says_unknown() {
        let outcome = finished(ErrorKind::SummaryMismatch);
        let datagram = record(
            None,
            Stage::Finished(&outcome),
            Some("update system (1 package)"),
        );
        assert_eq!(
            text(&datagram),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=4\n\
             TIDEWISE_CALLER_UID=unknown\n\
             TIDEWISE_STAGE=finished\n\
             TIDEWISE_OUTCOME=summary_mismatch\n\
             TIDEWISE_SUMMARY=update system (1 package)\n\
             MESSAGE=uid unknown: summary_mismatch: update system (1 package)\n"
        );
    }

    #[test]
    fn a_request_that_did_not_pass_validation_has_no_summary_line() {
        let outcome = finished(ErrorKind::Malformed);
        let datagram = record(Some(0), Stage::Finished(&outcome), None);
        assert_eq!(
            text(&datagram),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=4\n\
             TIDEWISE_CALLER_UID=0\n\
             TIDEWISE_STAGE=finished\n\
             TIDEWISE_OUTCOME=malformed\n\
             MESSAGE=uid 0: malformed\n"
        );
    }

    #[test]
    fn every_line_is_a_plain_key_equals_value_and_nothing_hides_in_a_value() {
        let hostile = "a\nMESSAGE=evil\0\u{1b}[31m caf\u{e9}\r\nPRIORITY=0\u{202e}";
        let mut cases: Vec<(Vec<u8>, usize)> =
            vec![(record(Some(1000), Stage::Accepted, Some(hostile)), 6)];
        for kind in all_error_kinds() {
            let outcome = finished(kind);
            cases.push((
                record(Some(1000), Stage::Finished(&outcome), Some(hostile)),
                7,
            ));
        }
        cases.push((
            record(Some(1000), Stage::Finished(&Outcome::Ok), Some(hostile)),
            7,
        ));
        for (datagram, expected_lines) in cases {
            let text = text(&datagram);
            assert!(text.ends_with('\n'));
            let lines: Vec<&str> = text.strip_suffix('\n').unwrap().split('\n').collect();
            assert_eq!(lines.len(), expected_lines, "{text:?}");
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
            // Text like "MESSAGE=evil" may survive inside a value (harmless: it is not at the start of a line),
            // but exactly one line may have the key MESSAGE and exactly one the key PRIORITY.
            for key in ["MESSAGE", "PRIORITY"] {
                let prefix = format!("{key}=");
                let starting = lines
                    .iter()
                    .filter(|line| line.starts_with(&prefix))
                    .count();
                assert_eq!(starting, 1, "{key} in {text:?}");
            }
        }
    }

    #[test]
    fn a_non_ascii_character_becomes_one_question_mark() {
        let datagram = record(Some(1), Stage::Accepted, Some("caf\u{e9}"));
        assert!(text(&datagram).contains("TIDEWISE_SUMMARY=caf?\n"));
    }

    #[test]
    fn only_a_refusal_is_a_warning() {
        let mut stages: Vec<(Outcome, &str)> = vec![(Outcome::Ok, "PRIORITY=6\n")];
        for kind in all_error_kinds() {
            stages.push((finished(kind), "PRIORITY=4\n"));
        }
        for (outcome, expected) in stages {
            let datagram = record(Some(1), Stage::Finished(&outcome), None);
            assert!(text(&datagram).contains(expected), "{outcome:?}");
        }
        assert!(text(&record(Some(1), Stage::Accepted, None)).contains("PRIORITY=6\n"));
    }

    #[test]
    fn every_outcome_has_its_own_snake_case_code() {
        let mut outcomes = vec![Outcome::Ok];
        outcomes.extend(all_error_kinds().into_iter().map(Outcome::Error));
        let codes: Vec<&str> = outcomes.iter().map(finished_code).collect();
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
        assert_eq!(finished_code(&Outcome::Ok), "ok");
        assert_eq!(
            outcome_code(&ErrorKind::SummaryMismatch),
            "summary_mismatch"
        );
    }

    #[test]
    fn the_raw_request_and_the_argv_text_never_reach_the_entry() {
        let request = r#"{"protocol_version":1,"action":{"update_system":{"transaction":[{"repo":"core","name":"SECRET-PATH/etc/shadow","version":"1-1"}]}}}"#;
        let crate::gate::Decision::Refused(verdict) =
            crate::gate::decide(b"SECRET-ARGV", Ok(request.as_bytes().to_vec()))
        else {
            panic!("the request should have been refused");
        };
        let outcome = Outcome::Error(verdict.error.clone());
        let datagram = text(&record(
            Some(1000),
            Stage::Finished(&outcome),
            verdict.summary.as_deref(),
        ));
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
            Stage::Accepted,
            Some("update system (3 packages)"),
        );
        assert!(!datagram.is_empty());
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
