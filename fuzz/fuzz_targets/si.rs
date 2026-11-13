#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // run.rs decodes pacman's bytes lossily, so the parser only ever sees valid text; here we skip
    // non-UTF-8 input instead (the lossy step has its own unit test).
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // Ok and Err are both fine; only a panic (or hang) is a bug.
    let _ = tidewise_pacman::parse_si(text);
});
