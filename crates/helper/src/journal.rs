//! Step 10: one journal entry per request that got past the start checks, and one for a start refusal that comes after
//! the root check (threat T-14, open questions 7 and 27).
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

use crate::flows::ListsState;

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
    /// The end: the result the final line will carry (a refused request has only this entry), and what the notices
    /// said about the run.
    Finished(&'a Outcome, Notes),
    /// A start check that comes after the root check refused (threat model question 27). Only a fixed code: the
    /// `'static` lifetime means no runtime text, argv included, can ever be passed in.
    StartRefused(&'static str),
}

/// What a job's notices said, for the journal: a clean `ok` and an `ok` that came with something to read must never look
/// alike, and a check that did not run must never look like a check that passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Notes {
    /// Confirmed items not installed at the confirmed version after the run.
    pub not_installed: u32,
    /// Packages that were installed before the real run and are gone after it.
    pub removed: u32,
    /// The "did a package disappear?" check could not run (a `pacman -Q` list could not be read).
    pub removal_check_skipped: bool,
    /// Output lines the helper did not queue for the app because the app was not reading them (review 5.8 R-04). A
    /// diagnostic, not a notice: it is **not** part of `any()`, so an `ok` that dropped progress lines is still `ok`.
    pub events_dropped: u32,
    /// Only for an update: what happened to the REAL package lists (decision 0007). pacman does not log the move, so
    /// this is the record of it. A diagnostic like `events_dropped`: **not** part of `any()`, so an `ok` that moved the
    /// lists is still a clean `ok`. `None` (a plain install) writes no field.
    pub lists: Option<ListsState>,
}

impl Notes {
    /// True if there is anything to say.
    pub fn any(&self) -> bool {
        self.not_installed > 0 || self.removed > 0 || self.removal_check_skipped
    }
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
    // syslog levels: 6 = info (accepted, or a clean finish), 4 = warning (a refusal, a failure, or an ok that came
    // with something to read or with a check that did not run).
    let (priority, stage_name, code, notes) = match stage {
        Stage::Accepted => ("6", "accepted", None, Notes::default()),
        // A warning with the fixed code of the refusal: who asked and why it did not start, never what they typed.
        Stage::StartRefused(code) => ("4", "start_refused", Some(code), Notes::default()),
        Stage::Finished(outcome, notes) => {
            let ok_with_notices = matches!(outcome, Outcome::Ok) && notes.any();
            let clean_ok = matches!(outcome, Outcome::Ok) && !notes.any();
            let code = if ok_with_notices {
                "ok_with_notices"
            } else {
                finished_code(outcome)
            };
            (
                if clean_ok { "6" } else { "4" },
                "finished",
                Some(code),
                notes,
            )
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
    if notes.not_installed > 0 {
        add_field(
            &mut text,
            "TIDEWISE_NOT_INSTALLED",
            &notes.not_installed.to_string(),
        );
    }
    if notes.removed > 0 {
        add_field(&mut text, "TIDEWISE_REMOVED", &notes.removed.to_string());
    }
    if notes.removal_check_skipped {
        add_field(&mut text, "TIDEWISE_REMOVAL_CHECK", "skipped");
    }
    if notes.events_dropped > 0 {
        add_field(
            &mut text,
            "TIDEWISE_EVENTS_DROPPED",
            &notes.events_dropped.to_string(),
        );
    }
    if let Some(lists) = notes.lists {
        let word = match lists {
            ListsState::Moved => "moved",
            ListsState::Untouched => "untouched",
        };
        add_field(&mut text, "TIDEWISE_LISTS", word);
    }

    let label = match code {
        Some("ok_with_notices") => format!(
            "ok with notices ({} not installed, {} removed{})",
            notes.not_installed,
            notes.removed,
            if notes.removal_check_skipped {
                ", removal check skipped"
            } else {
                ""
            }
        ),
        Some(code) if matches!(stage, Stage::StartRefused(_)) => {
            format!("refused to start: {code}")
        }
        Some(code) => code.to_string(),
        None => "accepted".to_string(),
    };
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
    use crate::testing::TempDir;
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
            Stage::Finished(&Outcome::Ok, Notes::default()),
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

    fn notes(not_installed: u32, removed: u32, skipped: bool) -> Notes {
        Notes {
            not_installed,
            removed,
            removal_check_skipped: skipped,
            events_dropped: 0,
            lists: None,
        }
    }

    #[test]
    fn the_lists_field_is_moved_or_untouched_for_an_update_and_absent_otherwise() {
        let none = finish_with(&Outcome::Ok, notes(0, 0, false));
        assert!(!none.contains("TIDEWISE_LISTS"), "{none}");
        for (state, word) in [
            (ListsState::Moved, "moved"),
            (ListsState::Untouched, "untouched"),
        ] {
            let with = Notes {
                lists: Some(state),
                ..notes(0, 0, false)
            };
            let text = finish_with(&Outcome::Ok, with);
            assert!(text.contains(&format!("TIDEWISE_LISTS={word}\n")), "{text}");
            // A diagnostic, not a notice: still a clean ok.
            assert!(text.contains("TIDEWISE_OUTCOME=ok\n"), "{text}");
            assert!(text.contains("PRIORITY=6\n"), "{text}");
            assert!(!with.any());
        }
    }

    #[test]
    fn the_lists_field_is_there_for_a_refusal_too_and_never_on_the_accepted_entry() {
        let refused = Outcome::Error(ErrorKind::TransactionChanged);
        let with = Notes {
            lists: Some(ListsState::Untouched),
            ..notes(0, 0, false)
        };
        let entry = finish_with(&refused, with);
        assert!(entry.contains("TIDEWISE_LISTS=untouched\n"), "{entry}");
        assert!(
            entry.contains("TIDEWISE_OUTCOME=transaction_changed\n"),
            "{entry}"
        );
        let accepted = text(&record(
            Some(1000),
            Stage::Accepted,
            Some("update system (3 packages)"),
        ));
        assert!(!accepted.contains("TIDEWISE_LISTS"), "{accepted}");
    }

    #[test]
    fn dropped_lines_are_a_field_only_above_zero_and_never_make_an_ok_a_notice() {
        let none = finish_with(&Outcome::Ok, notes(0, 0, false));
        assert!(!none.contains("TIDEWISE_EVENTS_DROPPED"), "{none}");

        let dropped = Notes {
            events_dropped: 1978,
            ..notes(0, 0, false)
        };
        let text = finish_with(&Outcome::Ok, dropped);
        assert!(text.contains("TIDEWISE_EVENTS_DROPPED=1978\n"), "{text}");
        // Still a clean ok: the same code, the same priority, no "with notices".
        assert!(text.contains("TIDEWISE_OUTCOME=ok\n"), "{text}");
        assert!(text.contains("PRIORITY=6\n"), "{text}");
        assert!(!dropped.any());
    }

    fn finish_with(outcome: &Outcome, notes: Notes) -> String {
        text(&record(
            Some(1000),
            Stage::Finished(outcome, notes),
            Some("update system (3 packages)"),
        ))
    }

    #[test]
    fn an_ok_with_packages_not_installed_says_ok_with_notices_and_is_a_warning() {
        assert_eq!(
            finish_with(&Outcome::Ok, notes(1, 0, false)),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=4\n\
             TIDEWISE_CALLER_UID=1000\n\
             TIDEWISE_STAGE=finished\n\
             TIDEWISE_OUTCOME=ok_with_notices\n\
             TIDEWISE_NOT_INSTALLED=1\n\
             TIDEWISE_SUMMARY=update system (3 packages)\n\
             MESSAGE=uid 1000: ok with notices (1 not installed, 0 removed): update system (3 packages)\n"
        );
    }

    #[test]
    fn an_ok_with_removed_packages_says_how_many() {
        assert_eq!(
            finish_with(&Outcome::Ok, notes(0, 2, false)),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=4\n\
             TIDEWISE_CALLER_UID=1000\n\
             TIDEWISE_STAGE=finished\n\
             TIDEWISE_OUTCOME=ok_with_notices\n\
             TIDEWISE_REMOVED=2\n\
             TIDEWISE_SUMMARY=update system (3 packages)\n\
             MESSAGE=uid 1000: ok with notices (0 not installed, 2 removed): update system (3 packages)\n"
        );
    }

    #[test]
    fn a_check_that_did_not_run_is_never_logged_like_a_clean_pass() {
        // The human's rule for 5.3c: a skipped removal check is visible, and an otherwise ok result is a warning.
        assert_eq!(
            finish_with(&Outcome::Ok, notes(0, 0, true)),
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=4\n\
             TIDEWISE_CALLER_UID=1000\n\
             TIDEWISE_STAGE=finished\n\
             TIDEWISE_OUTCOME=ok_with_notices\n\
             TIDEWISE_REMOVAL_CHECK=skipped\n\
             TIDEWISE_SUMMARY=update system (3 packages)\n\
             MESSAGE=uid 1000: ok with notices (0 not installed, 0 removed, removal check skipped): update system (3 packages)\n"
        );
    }

    #[test]
    fn all_three_notes_come_in_a_fixed_order() {
        let entry = finish_with(&Outcome::Ok, notes(3, 4, true));
        assert!(
            entry.contains(
                "TIDEWISE_OUTCOME=ok_with_notices\nTIDEWISE_NOT_INSTALLED=3\nTIDEWISE_REMOVED=4\nTIDEWISE_REMOVAL_CHECK=skipped\nTIDEWISE_SUMMARY="
            ),
            "{entry}"
        );
        assert!(
            entry.contains("(3 not installed, 4 removed, removal check skipped)"),
            "{entry}"
        );
    }

    #[test]
    fn a_clean_ok_is_exactly_what_it_was_and_has_no_notes_fields() {
        let entry = finish_with(&Outcome::Ok, Notes::default());
        for field in [
            "NOT_INSTALLED",
            "REMOVED",
            "REMOVAL_CHECK",
            "ok_with_notices",
        ] {
            assert!(!entry.contains(field), "{entry}");
        }
        assert!(
            entry.contains("PRIORITY=6\n") && entry.contains("TIDEWISE_OUTCOME=ok\n"),
            "{entry}"
        );
        assert!(!Notes::default().any());
    }

    #[test]
    fn an_error_keeps_its_own_code_and_gets_the_notes_as_fields() {
        let outcome = Outcome::Error(ErrorKind::Malformed);
        let entry = finish_with(&outcome, notes(5, 0, true));
        assert!(entry.contains("TIDEWISE_OUTCOME=malformed\nTIDEWISE_NOT_INSTALLED=5\nTIDEWISE_REMOVAL_CHECK=skipped\n"), "{entry}");
        assert!(entry.contains("PRIORITY=4\n"), "{entry}");
        assert!(
            entry.contains("MESSAGE=uid 1000: malformed: update system (3 packages)\n"),
            "{entry}"
        );
        assert!(!entry.contains("ok_with_notices"), "{entry}");
    }

    #[test]
    fn a_big_count_is_just_a_number() {
        let entry = finish_with(&Outcome::Ok, notes(u32::MAX, u32::MAX, false));
        assert!(
            entry.contains("TIDEWISE_NOT_INSTALLED=4294967295\nTIDEWISE_REMOVED=4294967295\n"),
            "{entry}"
        );
    }

    #[test]
    fn a_refusal_is_a_warning_and_an_unknown_caller_says_unknown() {
        let outcome = finished(ErrorKind::SummaryMismatch);
        let datagram = record(
            None,
            Stage::Finished(&outcome, Notes::default()),
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
        let datagram = record(Some(0), Stage::Finished(&outcome, Notes::default()), None);
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
                record(
                    Some(1000),
                    Stage::Finished(&outcome, Notes::default()),
                    Some(hostile),
                ),
                7,
            ));
        }
        cases.push((
            record(
                Some(1000),
                Stage::Finished(&Outcome::Ok, Notes::default()),
                Some(hostile),
            ),
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
            let datagram = record(Some(1), Stage::Finished(&outcome, Notes::default()), None);
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
            Stage::Finished(&outcome, Notes::default()),
            verdict.summary.as_deref(),
        ));
        assert!(!datagram.contains("SECRET"), "{datagram}");
        assert!(
            datagram.contains("TIDEWISE_OUTCOME=invalid\n"),
            "{datagram}"
        );
    }

    // ---- a refused start ----------------------------------------------------------------------------------

    #[test]
    fn a_start_refusal_is_a_warning_with_a_fixed_code_the_caller_and_no_summary() {
        let entry = String::from_utf8(record(
            Some(1000),
            Stage::StartRefused("wrong_program_path"),
            None,
        ))
        .unwrap();
        assert_eq!(
            entry,
            "SYSLOG_IDENTIFIER=tidewise-helper\n\
             PRIORITY=4\n\
             TIDEWISE_CALLER_UID=1000\n\
             TIDEWISE_STAGE=start_refused\n\
             TIDEWISE_OUTCOME=wrong_program_path\n\
             MESSAGE=uid 1000: refused to start: wrong_program_path\n"
        );
    }

    #[test]
    fn a_start_refusal_with_an_unknown_caller_says_unknown() {
        let entry =
            String::from_utf8(record(None, Stage::StartRefused("unexpected_fd"), None)).unwrap();
        assert!(entry.contains("TIDEWISE_CALLER_UID=unknown\n"), "{entry}");
        assert!(entry.contains("PRIORITY=4\n"), "{entry}");
        assert!(
            entry.contains("TIDEWISE_OUTCOME=unexpected_fd\n"),
            "{entry}"
        );
    }

    // ---- sending ----------------------------------------------------------------------------------------

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
