#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Some(entry) = tidewise_adapters::desktop::parse_desktop_file(s) {
            let _ = tidewise_adapters::desktop::exec_program(&entry.exec);
        }
        let _ = tidewise_adapters::desktop::exec_program(s);
        let _ = tidewise_adapters::desktop::parse_pacman_qo(s);
        let _ = tidewise_adapters::desktop::parse_dpkg_s(s);
    }
});
