#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = tidewise_adapters::dnf::parse_rpm_qa(s);
        let _ = tidewise_adapters::dnf::parse_dry_run(s);
    }
});
