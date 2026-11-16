#![no_main]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

use libfuzzer_sys::fuzz_target;
use tidewise_helper::args::{ArgsError, HELPER_PATH, caller_uid, summary_arg};
use tidewise_helper::gate::{Decision, decide};
use tidewise_helper::input::InputError;
use tidewise_helper_protocol::{MAX_SUMMARY_BYTES, parse_request, summary};

// The three things a caller chooses besides the request itself: argv (the summary text and the program name) and
// the `PKEXEC_UID` variable. All of it is hostile (threat model A1), and it all reaches the gate. The input is cut
// as: byte 0 = length of the argv text, byte 1 = length of the id text (both capped by what is left), then the
// argv text, then the id text, then the rest is the request on stdin. Ok and Err are both fine; a panic, a hang,
// or a broken promise below is a bug.
fuzz_target!(|data: &[u8]| {
    let (Some(&first), Some(&second)) = (data.first(), data.get(1)) else {
        return;
    };
    let rest = data.get(2..).unwrap_or_default();
    let argv_len = usize::from(first).min(rest.len());
    let (argv_text, rest) = rest.split_at(argv_len);
    let uid_len = usize::from(second % 16).min(rest.len());
    let (uid_text, request) = rest.split_at(uid_len);

    // 1. argv: exactly [HELPER_PATH, text] gives the text back, byte for byte; anything else is refused with a code.
    let right = vec![
        OsString::from(HELPER_PATH),
        OsString::from_vec(argv_text.to_vec()),
    ];
    assert_eq!(summary_arg(right), Ok(argv_text.to_vec()));
    let wrong_name = vec![
        OsString::from_vec(argv_text.to_vec()),
        OsString::from_vec(argv_text.to_vec()),
    ];
    if argv_text != HELPER_PATH.as_bytes() {
        assert_eq!(summary_arg(wrong_name), Err(ArgsError::WrongProgramPath));
    }
    assert_eq!(
        summary_arg([OsString::from(HELPER_PATH)]),
        Err(ArgsError::WrongCount)
    );
    assert_eq!(
        summary_arg([
            OsString::from(HELPER_PATH),
            OsString::from_vec(argv_text.to_vec()),
            OsString::from_vec(argv_text.to_vec()),
        ]),
        Err(ArgsError::WrongCount)
    );

    // 2. PKEXEC_UID: a number only for plain decimal digits, and then the very number that was written.
    if let Some(number) = caller_uid(Some(OsString::from_vec(uid_text.to_vec()))) {
        assert!(uid_text.iter().all(u8::is_ascii_digit), "not digits");
        assert_eq!(number.to_string().trim_start_matches('0'), {
            let text = std::str::from_utf8(uid_text).unwrap_or_default();
            text.trim_start_matches('0')
        });
    }

    // 3. The decision. Accepted only when the request is valid and argv is exactly the recomputed summary; a refusal
    //    carries at most the recomputed summary (printable ASCII, capped), never the argv text as such.
    match decide(argv_text, Ok(request.to_vec())) {
        Decision::Accepted {
            request: accepted,
            summary: text,
        } => {
            assert_eq!(text.as_bytes(), argv_text, "accepted with another summary");
            assert_eq!(text, summary(&accepted));
            assert!(parse_request(request).is_ok(), "accepted but not valid");
        }
        Decision::Refused(verdict) => {
            if let Some(text) = verdict.summary {
                assert!(text.len() <= MAX_SUMMARY_BYTES);
                assert!(text.bytes().all(|b| (0x20..=0x7e).contains(&b)));
                assert_ne!(text.as_bytes(), argv_text, "a match was refused");
                let parsed = parse_request(request).expect("a summary without a valid request");
                assert_eq!(text, summary(&parsed));
            }
        }
    }
    // A request that could not be read is never accepted.
    assert!(matches!(
        decide(argv_text, Err(InputError::ReadFailed)),
        Decision::Refused(_)
    ));
});
