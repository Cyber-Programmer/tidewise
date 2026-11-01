#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = tidewise_adapters::flatpak::parse_list(s);
        if let (Ok(runtimes), Ok(apps)) = (
            tidewise_adapters::flatpak::parse_runtimes(s),
            tidewise_adapters::flatpak::parse_app_runtimes(s),
        ) {
            let _ = tidewise_adapters::flatpak::unused_runtimes(&runtimes, &apps);
        }
    }
});
