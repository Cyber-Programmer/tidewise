//! `parse_request`: the one door every request goes through (threat model T-01, T-07).

use crate::{InvalidRequest, Request};

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

/// Turns the bytes read from stdin into a validated request, or says why not.
///
/// STUB (red commit): refuses everything as malformed.
pub fn parse_request(_bytes: &[u8]) -> Result<Request, ProtocolError> {
    Err(ProtocolError::Malformed)
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
    fn deep_nesting_is_malformed_and_does_not_overflow_the_stack() {
        let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
        for text in [
            deep.clone(),
            format!(r#"{{"protocol_version":1,"action":{deep}}}"#),
            format!(r#"{{"protocol_version":2,"unknown":{deep}}}"#),
            format!(
                r#"{{"protocol_version":1,"action":{{"update_system":{{"transaction":{deep}}}}}}}"#
            ),
        ] {
            assert_eq!(parse(&text), Err(ProtocolError::Malformed));
        }
    }

    #[test]
    fn serde_jsons_nesting_limit_sits_between_100_and_200_levels() {
        // Threat model open question 8: serde_json stops at 128 levels (believed, now pinned by this test).
        // 100 levels are read (the version is then judged), 200 are refused as malformed.
        let shallow = format!("{}{}", "[".repeat(100), "]".repeat(100));
        let text = format!(r#"{{"protocol_version":2,"unknown":{shallow}}}"#);
        assert_eq!(parse(&text), Err(ProtocolError::VersionMismatch));
        let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
        let text = format!(r#"{{"protocol_version":2,"unknown":{deep}}}"#);
        assert_eq!(parse(&text), Err(ProtocolError::Malformed));
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
