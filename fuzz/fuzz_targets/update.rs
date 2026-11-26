#![no_main]

use libfuzzer_sys::fuzz_target;

// The update review's reading of outside data (WBS 6.5): four pacman texts, split at NUL bytes. `update_plan` reads
// them as -Qu, the dry run, -Si and -Qi (its own reader of Provides and Conflicts With included), and `held_block`
// reads -Qu and the two streams of a failed dry run. Ok and Err are both fine; only a panic (or hang) is a bug.
fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mut parts = text.split('\0');
    let mut next = || parts.next().unwrap_or("");
    let (qu, print, si, qi) = (next(), next(), next(), next());
    if let Ok(plan) = tidewise_pacman::update_plan(qu, print, si, qi) {
        // The sums the screen shows must not panic either.
        let _ = (plan.download_total(), plan.size_change());
        assert!(plan.installing().count() <= plan.items.len());
    }
    let _ = tidewise_pacman::held_block(qu, print, si);
});
