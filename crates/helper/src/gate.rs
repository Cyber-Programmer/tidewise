//! Steps 7 to 9: the decision (threats T-01, T-02, T-04 to T-07).
//!
//! A pure function from "the bytes of argv[1]" and "what was read from stdin" to a verdict. No I/O, no root, so every
//! branch is a unit test. The order matters and is tested: the request is parsed and validated first, and only a
//! request that passed is compared with the summary, so an attacker cannot learn anything from the summary check
//! about a request that was not valid.
//!
//! In 5.3a a valid request with the right summary ends in `NotImplemented`: there are no jobs yet.

use tidewise_helper_protocol::{ErrorKind, parse_request, summary};

use crate::input::InputError;

/// What the gate decided, and what may go in the log about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Why the request goes no further. (In 5.3a every verdict is an error; 5.3b adds the success case.)
    pub error: ErrorKind,
    /// The summary the helper **recomputed** from a request that passed validation. `None` when the request did not
    /// pass. Never the argv text and never anything from the raw request.
    pub summary: Option<String>,
}

/// Decides what to do with a request.
pub fn decide(summary_arg: &[u8], input: Result<Vec<u8>, InputError>) -> Verdict {
    // A request that could not be read is treated like one that could not be understood.
    let Ok(bytes) = input else {
        return refused(ErrorKind::Malformed);
    };
    let request = match parse_request(&bytes) {
        Ok(request) => request,
        Err(error) => return refused(ErrorKind::from(error)),
    };

    // Only a request that passed validation is compared with argv. The text we compare against is the one we
    // compute ourselves; argv is never used for anything else.
    let expected = summary(&request);
    let error = if expected.as_bytes() == summary_arg {
        ErrorKind::NotImplemented
    } else {
        ErrorKind::SummaryMismatch
    };
    Verdict {
        error,
        summary: Some(expected),
    }
}

/// A verdict for a request that did not pass validation: no summary, nothing from the request.
fn refused(error: ErrorKind) -> Verdict {
    Verdict {
        error,
        summary: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_helper_protocol::MAX_REQUEST_BYTES;

    const INSTALL: &str = include_str!("../../../tests/fixtures/helper-protocol/install.json");
    const UPDATE: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
    const BOTH: &str =
        include_str!("../../../tests/fixtures/helper-protocol/update-system-and-install.json");

    const INSTALL_SUMMARY: &str = "install extra/dbeaver with extra/jre-openjdk (5 packages)";
    const UPDATE_SUMMARY: &str = "update system (3 packages)";
    const BOTH_SUMMARY: &str = "update system and install extra/vlc (2 packages)";

    fn run(summary_arg: &str, request: &str) -> Verdict {
        decide(summary_arg.as_bytes(), Ok(request.as_bytes().to_vec()))
    }

    // ---- the good path ----------------------------------------------------------------------------------

    #[test]
    fn each_sample_request_with_its_own_summary_ends_in_not_implemented() {
        for (request, text) in [
            (INSTALL, INSTALL_SUMMARY),
            (UPDATE, UPDATE_SUMMARY),
            (BOTH, BOTH_SUMMARY),
        ] {
            assert_eq!(
                run(text, request),
                Verdict {
                    error: ErrorKind::NotImplemented,
                    summary: Some(text.to_string())
                }
            );
        }
    }

    // ---- the summary check (T-02) -----------------------------------------------------------------------

    #[test]
    fn a_summary_that_is_wrong_in_any_way_is_a_mismatch() {
        let one_off = format!("{}x", &UPDATE_SUMMARY[..UPDATE_SUMMARY.len() - 1]);
        let wrong = [
            "",
            " ",
            "update system",
            "update system (3 packages) ",
            " update system (3 packages)",
            "update system (3 packages)\n",
            "update system (3 packages)\0",
            "Update system (3 packages)",
            "update system (4 packages)",
            "update  system (3 packages)",
            INSTALL_SUMMARY,
            BOTH_SUMMARY,
            one_off.as_str(),
        ];
        for text in wrong {
            assert_eq!(
                run(text, UPDATE),
                Verdict {
                    error: ErrorKind::SummaryMismatch,
                    summary: Some(UPDATE_SUMMARY.to_string())
                },
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_summary_that_is_not_text_is_a_mismatch() {
        let verdict = decide(&[0xff, 0xfe, 0x00], Ok(UPDATE.as_bytes().to_vec()));
        assert_eq!(verdict.error, ErrorKind::SummaryMismatch);
    }

    #[test]
    fn the_log_gets_the_recomputed_summary_never_the_argv_text() {
        let verdict = run("SECRET-ARGV-TEXT", UPDATE);
        assert_eq!(verdict.summary.as_deref(), Some(UPDATE_SUMMARY));
        assert!(!format!("{verdict:?}").contains("SECRET"));
    }

    // ---- validation comes first (T-01, T-04..T-07) ------------------------------------------------------

    fn too_big() -> Vec<u8> {
        vec![b' '; MAX_REQUEST_BYTES + 1]
    }

    #[test]
    fn requests_that_fail_are_refused_with_their_code() {
        let v2 = UPDATE.replace("\"protocol_version\":1", "\"protocol_version\":2");
        let force = UPDATE.replace("\"name\":\"glibc\"", "\"name\":\"--force\"");
        let cases: Vec<(Vec<u8>, ErrorKind)> = vec![
            (too_big(), ErrorKind::TooLarge),
            (b"".to_vec(), ErrorKind::Malformed),
            (b"{".to_vec(), ErrorKind::Malformed),
            (b"[]".to_vec(), ErrorKind::Malformed),
            (vec![0xff, 0xfe], ErrorKind::Malformed),
            (format!("{UPDATE}x").into_bytes(), ErrorKind::Malformed),
            (v2.into_bytes(), ErrorKind::VersionMismatch),
        ];
        for (bytes, expected) in cases {
            let verdict = decide(UPDATE_SUMMARY.as_bytes(), Ok(bytes));
            assert_eq!(verdict.error, expected);
            assert_eq!(verdict.summary, None);
        }
        let verdict = run(UPDATE_SUMMARY, &force);
        assert!(
            matches!(verdict.error, ErrorKind::Invalid(_)),
            "{verdict:?}"
        );
        assert_eq!(verdict.summary, None);
    }

    #[test]
    fn a_read_failure_is_a_malformed_request() {
        let verdict = decide(UPDATE_SUMMARY.as_bytes(), Err(InputError::ReadFailed));
        assert_eq!(
            verdict,
            Verdict {
                error: ErrorKind::Malformed,
                summary: None
            }
        );
    }

    #[test]
    fn a_bad_request_gets_the_same_answer_whatever_argv_says() {
        // The summary is only compared for a request that passed, so argv cannot change the answer for one that
        // did not (and cannot be used to probe it).
        let force = UPDATE.replace("\"name\":\"glibc\"", "\"name\":\"--force\"");
        for bad in [b"{".to_vec(), too_big(), force.into_bytes()] {
            let answers: Vec<Verdict> = ["", "anything", UPDATE_SUMMARY, INSTALL_SUMMARY]
                .iter()
                .map(|arg| decide(arg.as_bytes(), Ok(bad.clone())))
                .collect();
            assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
            assert!(!matches!(
                answers[0].error,
                ErrorKind::SummaryMismatch | ErrorKind::NotImplemented
            ));
        }
    }

    #[test]
    fn nothing_from_the_request_reaches_the_verdict_text() {
        let hostile = UPDATE.replace("\"name\":\"glibc\"", "\"name\":\"SECRET-PATH/etc/shadow\"");
        let verdict = run(UPDATE_SUMMARY, &hostile);
        assert!(matches!(verdict.error, ErrorKind::Invalid(_)));
        assert!(!format!("{verdict:?}").contains("SECRET"));
    }

    #[test]
    fn the_hostile_names_of_the_protocol_table_never_get_as_far_as_the_summary_check() {
        for name in [
            "-U",
            "../x",
            "a b",
            "*",
            "/tmp/a.pkg.tar.zst",
            "caf\u{e9}",
            "",
        ] {
            let request = UPDATE.replace("\"name\":\"glibc\"", &format!("\"name\":{name:?}"));
            let verdict = run(UPDATE_SUMMARY, &request);
            assert!(
                matches!(verdict.error, ErrorKind::Invalid(_)),
                "{name:?}: {verdict:?}"
            );
        }
    }
}
