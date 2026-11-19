#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // Ok and Err are both fine; a panic (or hang) is a bug.
    let Ok(packages) = tidewise_pacman::parse_print(text) else {
        return;
    };
    // What was accepted must be exactly what was written: writing the lines back gives the input
    // (apart from the one optional final newline), so nothing was dropped, trimmed or "cleaned".
    let again: Vec<String> = packages
        .iter()
        .map(|p| format!("{}/{} {} {}", p.repo, p.name, p.version, p.download_size))
        .collect();
    let body = text.strip_suffix('\n').unwrap_or(text);
    assert_eq!(again.join("\n"), body);
    // One package of each name.
    let mut names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), packages.len());
});
