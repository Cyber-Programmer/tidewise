#![no_main]
use libfuzzer_sys::fuzz_target;
use tidewise_core::search::{search, Group, Item};

// The first half of the input is the query, the second half becomes the title and keywords of a
// few items: neither may panic or give more than the allowed number of results.
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let mut mid = s.len() / 2;
        while !s.is_char_boundary(mid) {
            mid -= 1;
        }
        let (query, rest) = s.split_at(mid);
        let items: Vec<Item> = [Group::Apps, Group::Processes, Group::Actions, Group::Settings]
            .iter()
            .map(|g| Item {
                group: *g,
                title: rest.to_string(),
                meta: rest.chars().rev().collect(),
                keywords: rest.to_string(),
                key: String::new(),
            })
            .collect();
        assert!(search(&items, query).len() <= tidewise_core::search::TOTAL);
    }
});
