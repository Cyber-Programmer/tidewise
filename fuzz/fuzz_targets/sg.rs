#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(members) = tidewise_pacman::parse_sg(text) else {
        return;
    };
    // Writing the accepted lines back gives the input (apart from the one optional final newline).
    let again: Vec<String> = members
        .iter()
        .map(|m| format!("{} {}", m.group, m.name))
        .collect();
    let body = text.strip_suffix('\n').unwrap_or(text);
    assert_eq!(again.join("\n"), body);
});
