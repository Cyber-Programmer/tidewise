#![no_main]

use libfuzzer_sys::fuzz_target;
use tidewise_helper_protocol::{
    Details, MAX_DETAILS_BYTES, MAX_SUMMARY_BYTES, Request, event_line, parse_event, parse_request,
    summary, validate,
};

fn assert_summary_is_safe(request: &Request) {
    let text = summary(request);
    assert!(text.len() <= MAX_SUMMARY_BYTES, "summary too long");
    assert!(
        text.bytes().all(|b| (0x20..=0x7e).contains(&b)),
        "summary is not printable ASCII"
    );
}

// Everything the helper reads (a request on stdin) and everything the app reads (an event line) goes through
// a parser of outside data. Ok and Err are both fine; a panic, a hang, or a broken promise below is a bug.
fuzz_target!(|data: &[u8]| {
    // 1. The one door. Whatever it accepts must pass the allow-list again and have a safe summary.
    if let Ok(request) = parse_request(data) {
        assert_eq!(validate(&request), Ok(()), "accepted but not valid");
        assert_summary_is_safe(&request);
    }

    // 2. Whatever serde alone accepts (before validation) must still give a safe summary: summary() is total.
    if let Ok(request) = serde_json::from_slice::<Request>(data) {
        assert_summary_is_safe(&request);
    }

    // 3. Helper output as the app reads it: a parsed event prints back as one line that parses to the same
    //    event, and its text is clean and capped.
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(event) = parse_event(text) {
            let line = event_line(&event);
            assert!(!line.contains('\n'), "event line has a new line");
            assert_eq!(parse_event(&line), Ok(event), "event did not round-trip");
        }
        let details = Details::from_raw(text);
        assert!(details.as_str().len() <= MAX_DETAILS_BYTES);
        assert!(
            details
                .as_str()
                .chars()
                .all(|c| !c.is_control() || c == '\n')
        );
    }
});
