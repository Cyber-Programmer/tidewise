#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // run.rs rejects non-UTF-8 before parsing, so the parser only ever sees text.
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // Ok and Err are both fine; only a panic (or hang) is a bug.
    let _ = tidewise_pacman::parse_qi(text);
});
