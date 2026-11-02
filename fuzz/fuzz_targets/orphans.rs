#![no_main]
use libfuzzer_sys::fuzz_target;

// Both halves of every manager's input: the list text and the sizes text.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let mut mid = s.len() / 2;
        while !s.is_char_boundary(mid) {
            mid -= 1;
        }
        let (a, b) = s.split_at(mid);
        let _ = tidewise_adapters::orphans::from_pacman(a, b);
        let _ = tidewise_adapters::orphans::from_apt(a, b);
        let _ = tidewise_adapters::orphans::from_dnf(a, b);
    }
});
