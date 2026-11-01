#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = tidewise_adapters::apt::parse_dpkg_query(s);
        let _ = tidewise_adapters::apt::parse_simulated_remove(s, true);
        let _ = tidewise_adapters::apt::parse_simulated_remove(s, false);
    }
});
