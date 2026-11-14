//! `parse_request`: the one door every request goes through (threat model T-01, T-07).

use serde::Deserialize;

use crate::{InvalidRequest, PROTOCOL_VERSION, Request, validate};

/// Biggest request the helper reads, in bytes. Checked before any parsing.
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

/// Why a request was refused. Codes only: nothing from the input is ever copied into an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    /// More than `MAX_REQUEST_BYTES`.
    TooLarge,
    /// Not exactly one JSON value of the right shape (bad UTF-8, trailing data, repeated or unknown keys,
    /// wrong types, nesting too deep).
    Malformed,
    /// Well-formed enough to read `protocol_version`, and it is not ours.
    VersionMismatch,
    /// Right shape and version, but it fails the allow-list.
    Invalid(InvalidRequest),
}

/// Only the version, read loosely (other keys ignored) to tell "an old or new app" from "garbage".
#[derive(Deserialize)]
struct VersionOnly {
    protocol_version: u32,
}

/// Turns the bytes read from stdin into a validated request, or says why not.
///
/// Order: size cap (before any parsing), strict JSON into the typed request (exactly one value, nothing after
/// it, unknown and repeated keys refused, nesting bounded by serde_json's recursion limit), protocol version,
/// then the allow-list. When the strict parse fails, a second loose read of just `protocol_version` decides
/// between `VersionMismatch` and `Malformed`, so an app from another version gets the right message.
pub fn parse_request(bytes: &[u8]) -> Result<Request, ProtocolError> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    let request: Request = match serde_json::from_slice(bytes) {
        Ok(request) => request,
        Err(_) => {
            return Err(match serde_json::from_slice::<VersionOnly>(bytes) {
                Ok(peek) if peek.protocol_version != PROTOCOL_VERSION => {
                    ProtocolError::VersionMismatch
                }
                _ => ProtocolError::Malformed,
            });
        }
    };
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch);
    }
    validate(&request).map_err(ProtocolError::Invalid)?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Field, Reason};

    const INSTALL: &str = include_str!("../../../tests/fixtures/helper-protocol/install.json");
    const UPDATE: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
    const BOTH: &str =
        include_str!("../../../tests/fixtures/helper-protocol/update-system-and-install.json");

    const MINIMAL: &str = r#"{"protocol_version":1,"action":{"update_system":{"transaction":[{"repo":"core","name":"glibc","version":"2.42-2"}]}}}"#;

    fn parse(text: &str) -> Result<Request, ProtocolError> {
        parse_request(text.as_bytes())
    }

    #[test]
    fn the_sample_requests_parse() {
        for text in [INSTALL, UPDATE, BOTH, MINIMAL] {
            assert!(parse(text).is_ok(), "should accept: {text}");
        }
    }

    #[test]
    fn whitespace_around_the_value_is_fine() {
        assert!(parse(&format!("  \n{MINIMAL}\n\n")).is_ok());
    }

    #[test]
    fn a_request_over_the_byte_cap_is_refused_before_parsing() {
        let too_big = vec![b' '; MAX_REQUEST_BYTES + 1];
        assert_eq!(parse_request(&too_big), Err(ProtocolError::TooLarge));
    }

    #[test]
    fn a_request_exactly_at_the_cap_is_read_not_refused_for_size() {
        let at_cap = vec![b' '; MAX_REQUEST_BYTES];
        assert_eq!(parse_request(&at_cap), Err(ProtocolError::Malformed));
    }

    #[test]
    fn empty_input_is_malformed() {
        assert_eq!(parse_request(b""), Err(ProtocolError::Malformed));
    }

    #[test]
    fn anything_after_the_value_is_malformed() {
        for tail in ["x", "{}", "\0", MINIMAL, ",", "]"] {
            assert_eq!(
                parse(&format!("{MINIMAL}{tail}")),
                Err(ProtocolError::Malformed),
                "tail {tail:?}"
            );
        }
    }

    #[test]
    fn bad_bytes_are_malformed() {
        let mut invalid_utf8 = MINIMAL.as_bytes().to_vec();
        invalid_utf8.insert(20, 0xff);
        assert_eq!(parse_request(&invalid_utf8), Err(ProtocolError::Malformed));

        let mut with_bom = vec![0xef, 0xbb, 0xbf];
        with_bom.extend_from_slice(MINIMAL.as_bytes());
        assert_eq!(parse_request(&with_bom), Err(ProtocolError::Malformed));
    }

    #[test]
    fn a_repeated_or_unknown_key_is_malformed() {
        for text in [
            r#"{"protocol_version":1,"protocol_version":1,"action":{"update_system":{"transaction":[]}}}"#,
            r#"{"protocol_version":1,"action":{"update_system":{"transaction":[]}},"extra":1}"#,
        ] {
            assert_eq!(parse(text), Err(ProtocolError::Malformed), "{text}");
        }
    }

    #[test]
    fn another_protocol_version_is_a_version_mismatch() {
        for version in ["0", "2", "4294967295"] {
            let text = MINIMAL.replace(
                r#""protocol_version":1"#,
                &format!(r#""protocol_version":{version}"#),
            );
            assert_eq!(
                parse(&text),
                Err(ProtocolError::VersionMismatch),
                "version {version}"
            );
        }
    }

    #[test]
    fn another_version_with_another_shape_is_still_a_version_mismatch() {
        let text = r#"{"protocol_version":2,"action":{"frobnicate":{"x":1}},"more":true}"#;
        assert_eq!(parse(text), Err(ProtocolError::VersionMismatch));
    }

    #[test]
    fn our_version_with_a_bad_shape_is_malformed() {
        let text = r#"{"protocol_version":1,"action":{"frobnicate":{"x":1}}}"#;
        assert_eq!(parse(text), Err(ProtocolError::Malformed));
    }

    #[test]
    fn a_version_that_is_not_a_plain_number_is_malformed() {
        for version in [
            "-1",
            "1.0",
            "1e9",
            r#""1""#,
            "null",
            "true",
            "[1]",
            "99999999999",
        ] {
            let text = MINIMAL.replace(
                r#""protocol_version":1"#,
                &format!(r#""protocol_version":{version}"#),
            );
            assert_eq!(
                parse(&text),
                Err(ProtocolError::Malformed),
                "version {version}"
            );
        }
    }

    #[test]
    fn a_missing_version_is_malformed() {
        let text = r#"{"action":{"update_system":{"transaction":[]}}}"#;
        assert_eq!(parse(text), Err(ProtocolError::Malformed));
    }

    #[test]
    fn deep_nesting_is_refused_and_does_not_overflow_the_stack() {
        let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        for text in [
            deep.clone(),
            format!(r#"{{"protocol_version":1,"action":{deep}}}"#),
            format!(r#"{{"protocol_version":1,"unknown":{deep}}}"#),
            format!(
                r#"{{"protocol_version":1,"action":{{"update_system":{{"transaction":{deep}}}}}}}"#
            ),
        ] {
            assert_eq!(parse(&text), Err(ProtocolError::Malformed));
        }
    }

    #[test]
    fn serde_json_does_not_stop_at_128_levels_but_skips_deep_values_without_recursion() {
        // Threat model open question 8. I believed serde_json refuses nesting past 128 levels. Measured with
        // serde_json 1.0.154: it does not. A deep value under an unknown key is skipped in a loop (no stack
        // use; about 150 ms at the 1 million levels a 2 MiB request can hold), and the typed parse never
        // recurses beyond the request's own shape, because it refuses an array where an object belongs.
        // So what protects us is the byte cap plus the shape, not a depth limit. A request that says
        // protocol_version 2 is therefore a VersionMismatch however deep its junk is.
        let deep = format!("{}{}", "[".repeat(1_000_000), "]".repeat(1_000_000));
        assert!(deep.len() + 64 < MAX_REQUEST_BYTES);
        let v2 = format!(r#"{{"protocol_version":2,"unknown":{deep}}}"#);
        assert_eq!(parse(&v2), Err(ProtocolError::VersionMismatch));
        let v1 = format!(r#"{{"protocol_version":1,"unknown":{deep}}}"#);
        assert_eq!(parse(&v1), Err(ProtocolError::Malformed));
    }

    #[test]
    fn a_request_that_fails_the_allow_list_says_where() {
        let text = MINIMAL.replace(r#""name":"glibc""#, r#""name":"--force""#);
        assert_eq!(
            parse(&text),
            Err(ProtocolError::Invalid(InvalidRequest {
                field: Field::Transaction,
                index: 0,
                reason: Reason::BadPackageName
            }))
        );
    }

    #[test]
    fn a_nul_written_as_an_escape_is_refused_as_a_name() {
        let text = MINIMAL.replace(r#""name":"glibc""#, r#""name":"gl\u0000ibc""#);
        assert!(matches!(
            parse(&text),
            Err(ProtocolError::Invalid(InvalidRequest {
                reason: Reason::BadPackageName,
                ..
            }))
        ));
    }

    #[test]
    fn errors_never_contain_the_input() {
        let text = MINIMAL.replace(r#""name":"glibc""#, r#""name":"SECRET-PATH/etc/shadow""#);
        let error = parse(&text).unwrap_err();
        assert!(!format!("{error:?}").contains("SECRET"));
    }
}
