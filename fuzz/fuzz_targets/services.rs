#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let units = tidewise_adapters::services::parse_units(s).unwrap_or_default();
        let files = tidewise_adapters::services::parse_unit_files(s).unwrap_or_default();
        let memory = tidewise_adapters::services::parse_memory(s).unwrap_or_default();
        let _ = tidewise_adapters::services::build(&units, &files, &memory);
    }
});
