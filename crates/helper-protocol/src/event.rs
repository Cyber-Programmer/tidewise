//! What the helper prints on stdout: one JSON object per line, progress and then a final result.
//!
//! Error kinds are codes with fixed plain-language texts. The only free text in any event is `Details` (pacman's
//! own words for a failed action), cleaned and capped, and the app shows it only under "Technical details"
//! (threat model T-15). Nothing here can carry a path the caller chose.

use serde::{Deserialize, Serialize, Serializer};

use crate::{InvalidRequest, ProtocolError};

/// Longest `Details` text, in bytes.
pub const MAX_DETAILS_BYTES: usize = 4096;
/// Longest event line the app reads, in bytes. Room for `Details` at its worst (every character escaped).
pub const MAX_EVENT_LINE_BYTES: usize = 16 * 1024;

/// One line of helper output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// Something is happening.
    Progress(Progress),
    /// The last line: it worked, or why not.
    Finished(Outcome),
}

/// How far along one phase is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub phase: Phase,
    pub current: u32,
    pub total: u32,
}

/// The stages of a pacman transaction (to be checked against real pacman output in 5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Checking,
    Downloading,
    Installing,
    Hooks,
}

/// The final result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Error(ErrorKind),
}

/// Why nothing (or not everything) happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The request was over the size cap.
    TooLarge,
    /// The request could not be read.
    Malformed,
    /// The app and the helper speak different protocol versions.
    VersionMismatch,
    /// The request failed the allow-list (where, not what).
    Invalid(InvalidRequest),
    /// The text in argv was not the summary of the request (T-02).
    SummaryMismatch,
    /// The helper's own dry run is not the list the user confirmed (T-09).
    TransactionChanged,
    /// A plain install was asked for while updates are pending (T-11).
    SystemOutOfDate,
    /// pacman's database lock is held by someone else (T-10).
    DatabaseLocked,
    /// pacman ran and failed; its words, cleaned and capped.
    PacmanFailed(Details),
}

/// pacman's own words for a failure. Control characters (except a new line) and invisible direction marks
/// are removed, and the text is cut to `MAX_DETAILS_BYTES` at a character boundary. Also applied when an
/// event is read back, so a `Details` value is always clean.
///
/// STUB (red commit): keeps the text as it is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub struct Details(String);

impl Details {
    pub fn from_raw(raw: &str) -> Self {
        Self(raw.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for Details {
    fn from(raw: String) -> Self {
        Self::from_raw(&raw)
    }
}

impl Serialize for Details {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl ErrorKind {
    /// The fixed plain-language text for the app to show. Same text whatever the request held.
    ///
    /// STUB (red commit): empty.
    pub fn plain_text(&self) -> &'static str {
        ""
    }
}

impl From<ProtocolError> for ErrorKind {
    fn from(error: ProtocolError) -> Self {
        match error {
            ProtocolError::TooLarge => Self::TooLarge,
            ProtocolError::Malformed => Self::Malformed,
            ProtocolError::VersionMismatch => Self::VersionMismatch,
            ProtocolError::Invalid(invalid) => Self::Invalid(invalid),
        }
    }
}

/// Why an event line could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventError {
    TooLarge,
    Malformed,
}

/// The line the helper prints for `event`, without the trailing new line.
pub fn event_line(event: &Event) -> String {
    // These types always serialize; an empty line (which the app cannot read) is the harmless fallback.
    serde_json::to_string(event).unwrap_or_default()
}

/// Reads one line of helper output.
///
/// STUB (red commit): reads nothing.
pub fn parse_event(_line: &str) -> Result<Event, EventError> {
    Err(EventError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Field, Reason};

    fn invalid() -> InvalidRequest {
        InvalidRequest {
            field: Field::Targets,
            index: 0,
            reason: Reason::BadPackageName,
        }
    }

    fn all_error_kinds() -> Vec<ErrorKind> {
        vec![
            ErrorKind::TooLarge,
            ErrorKind::Malformed,
            ErrorKind::VersionMismatch,
            ErrorKind::Invalid(invalid()),
            ErrorKind::SummaryMismatch,
            ErrorKind::TransactionChanged,
            ErrorKind::SystemOutOfDate,
            ErrorKind::DatabaseLocked,
            ErrorKind::PacmanFailed(Details::from_raw("error: failed to commit transaction")),
        ]
    }

    fn all_events() -> Vec<Event> {
        let mut events = vec![Event::Finished(Outcome::Ok)];
        for phase in [
            Phase::Checking,
            Phase::Downloading,
            Phase::Installing,
            Phase::Hooks,
        ] {
            events.push(Event::Progress(Progress {
                phase,
                current: 2,
                total: 5,
            }));
        }
        events.extend(
            all_error_kinds()
                .into_iter()
                .map(|kind| Event::Finished(Outcome::Error(kind))),
        );
        events
    }

    // ---- the wire format -------------------------------------------------------------------------------

    #[test]
    fn the_lines_look_like_this() {
        let progress = Event::Progress(Progress {
            phase: Phase::Downloading,
            current: 2,
            total: 5,
        });
        assert_eq!(
            event_line(&progress),
            r#"{"progress":{"phase":"downloading","current":2,"total":5}}"#
        );
        assert_eq!(
            event_line(&Event::Finished(Outcome::Ok)),
            r#"{"finished":"ok"}"#
        );
        assert_eq!(
            event_line(&Event::Finished(Outcome::Error(ErrorKind::DatabaseLocked))),
            r#"{"finished":{"error":"database_locked"}}"#
        );
        assert_eq!(
            event_line(&Event::Finished(Outcome::Error(ErrorKind::Invalid(
                invalid()
            )))),
            r#"{"finished":{"error":{"invalid":{"field":"targets","index":0,"reason":"bad_package_name"}}}}"#
        );
        assert_eq!(
            event_line(&Event::Finished(Outcome::Error(ErrorKind::PacmanFailed(
                Details::from_raw("error: failed")
            )))),
            r#"{"finished":{"error":{"pacman_failed":"error: failed"}}}"#
        );
    }

    #[test]
    fn every_event_survives_a_round_trip_on_one_line() {
        for event in all_events() {
            let line = event_line(&event);
            assert!(!line.is_empty());
            assert!(!line.contains('\n'), "{line}");
            assert_eq!(parse_event(&line), Ok(event.clone()), "{line}");
        }
    }

    #[test]
    fn a_protocol_error_becomes_the_matching_error_kind() {
        assert_eq!(
            ErrorKind::from(ProtocolError::TooLarge),
            ErrorKind::TooLarge
        );
        assert_eq!(
            ErrorKind::from(ProtocolError::Malformed),
            ErrorKind::Malformed
        );
        assert_eq!(
            ErrorKind::from(ProtocolError::VersionMismatch),
            ErrorKind::VersionMismatch
        );
        assert_eq!(
            ErrorKind::from(ProtocolError::Invalid(invalid())),
            ErrorKind::Invalid(invalid())
        );
    }

    // ---- reading lines ---------------------------------------------------------------------------------

    #[test]
    fn bad_lines_are_refused() {
        let ok = event_line(&Event::Finished(Outcome::Ok));
        for line in [
            "",
            "{}",
            "ok",
            r#"{"finished":"fine"}"#,
            r#"{"finished":"ok","more":1}"#,
            r#"{"progress":{"phase":"downloading","current":1,"total":2,"path":"/x"}}"#,
            r#"{"progress":{"phase":"burning","current":1,"total":2}}"#,
            r#"{"progress":{"phase":"hooks","current":-1,"total":2}}"#,
            r#"{"finished":{"error":"disk_on_fire"}}"#,
            &format!("{ok}{ok}"),
            &format!("{ok} x"),
        ] {
            assert_eq!(parse_event(line), Err(EventError::Malformed), "{line}");
        }
    }

    #[test]
    fn a_line_over_the_cap_is_too_large() {
        let line = " ".repeat(MAX_EVENT_LINE_BYTES + 1);
        assert_eq!(parse_event(&line), Err(EventError::TooLarge));
    }

    #[test]
    fn details_at_their_worst_still_fit_in_a_line() {
        // Every character needs an escape in JSON (a quote becomes two bytes, a new line becomes two).
        for worst in ["\"", "\n"] {
            let details = Details::from_raw(&worst.repeat(MAX_DETAILS_BYTES * 2));
            let event = Event::Finished(Outcome::Error(ErrorKind::PacmanFailed(details)));
            let line = event_line(&event);
            assert!(line.len() <= MAX_EVENT_LINE_BYTES, "{}", line.len());
            assert_eq!(parse_event(&line), Ok(event));
        }
    }

    // ---- fixed texts (T-15) ----------------------------------------------------------------------------

    #[test]
    fn every_error_has_a_fixed_plain_text() {
        for kind in all_error_kinds() {
            let text = kind.plain_text();
            assert!(!text.is_empty(), "{kind:?}");
            assert!(text.bytes().all(|b| (0x20..=0x7e).contains(&b)), "{text}");
            assert!(text.ends_with('.'), "{text}");
            assert!(!text.contains('/'), "no paths in {text}");
        }
    }

    #[test]
    fn the_text_does_not_depend_on_what_the_request_or_pacman_said() {
        let other = InvalidRequest {
            field: Field::Transaction,
            index: 9,
            reason: Reason::Duplicate,
        };
        assert_eq!(
            ErrorKind::Invalid(invalid()).plain_text(),
            ErrorKind::Invalid(other).plain_text()
        );
        assert_eq!(
            ErrorKind::PacmanFailed(Details::from_raw("one")).plain_text(),
            ErrorKind::PacmanFailed(Details::from_raw("/etc/shadow two")).plain_text()
        );
    }

    #[test]
    fn different_errors_do_not_share_a_text() {
        let kinds = all_error_kinds();
        for (i, a) in kinds.iter().enumerate() {
            for b in kinds.iter().skip(i + 1) {
                assert_ne!(a.plain_text(), b.plain_text(), "{a:?} / {b:?}");
            }
        }
    }

    #[test]
    fn a_changed_transaction_tells_the_user_to_finish_with_a_full_update() {
        // Decided with the human: the helper's `-Syu --print` refreshes the real package lists first, so a
        // refusal can leave "lists newer than packages" (partial-upgrade-prone). Say what to do.
        let text = ErrorKind::TransactionChanged.plain_text();
        assert!(text.contains("full system update"), "{text}");
        assert!(text.contains("Nothing was changed"), "{text}");
    }

    #[test]
    fn a_locked_database_says_to_wait_and_try_again() {
        let text = ErrorKind::DatabaseLocked.plain_text();
        assert!(text.contains("try again"), "{text}");
    }

    #[test]
    fn an_error_line_holds_no_path_or_text_from_the_request() {
        for kind in all_error_kinds() {
            if matches!(kind, ErrorKind::PacmanFailed(_)) {
                continue;
            }
            let line = event_line(&Event::Finished(Outcome::Error(kind)));
            assert!(line.is_ascii() && !line.contains('/'), "{line}");
        }
    }

    // ---- Details ---------------------------------------------------------------------------------------

    #[test]
    fn details_keep_plain_text_and_new_lines() {
        let text = "error: failed to commit transaction (conflicting files)\nerror: x";
        assert_eq!(Details::from_raw(text).as_str(), text);
    }

    #[test]
    fn details_lose_control_characters_and_invisible_marks() {
        let raw =
            "a\u{1b}[31mred\u{1b}[0m\rb\0c\u{7}d\u{7f}e\u{9b}f\u{202e}g\u{200b}h\u{feff}i\u{2028}j";
        assert_eq!(Details::from_raw(raw).as_str(), "a[31mred[0mbcdefghij");
    }

    #[test]
    fn details_are_cut_at_the_cap_on_a_character_boundary() {
        for unit in ["x", "\u{e9}", "\u{20ac}", "\u{1f600}"] {
            let details = Details::from_raw(&unit.repeat(MAX_DETAILS_BYTES));
            assert!(details.as_str().len() <= MAX_DETAILS_BYTES, "{unit}");
            assert!(details.as_str().len() > MAX_DETAILS_BYTES - 4, "{unit}");
            assert!(details.as_str().chars().all(|c| unit.starts_with(c)));
        }
    }

    #[test]
    fn a_megabyte_of_details_is_cut_not_copied() {
        let details = Details::from_raw(&"y".repeat(1_000_000));
        assert_eq!(details.as_str().len(), MAX_DETAILS_BYTES);
    }

    #[test]
    fn details_read_back_from_a_line_are_cleaned_again() {
        let line = r#"{"finished":{"error":{"pacman_failed":"a\u001b[0mb\u0000c"}}}"#;
        assert_eq!(
            parse_event(line),
            Ok(Event::Finished(Outcome::Error(ErrorKind::PacmanFailed(
                Details::from_raw("a[0mbc")
            ))))
        );
    }
}
