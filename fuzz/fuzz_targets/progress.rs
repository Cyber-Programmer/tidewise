#![no_main]

use libfuzzer_sys::fuzz_target;
use tidewise_helper::progress::ProgressTracker;
use tidewise_helper_protocol::Phase;

fn rank(phase: Phase) -> u8 {
    match phase {
        Phase::Checking => 1,
        Phase::Downloading => 2,
        Phase::Installing => 3,
        Phase::Hooks => 4,
    }
}

// pacman's stdout is outside data (a package's install script can print anything), so the tracker must keep its
// promises whatever it is fed: events carry the confirmed total, a counter never passes it, a phase only moves
// forward (and starts at 0), and inside a phase the counter goes up by exactly one.
// The first byte is the confirmed total (0 to 255); the rest is the text.
fuzz_target!(|data: &[u8]| {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    let total = u32::from(first);
    let mut tracker = ProgressTracker::new(total);
    let mut last: Option<(u8, u32)> = None;
    for line in text.split('\n') {
        let Some(event) = tracker.line(line) else {
            continue;
        };
        assert_eq!(event.total, total);
        assert!(event.current <= event.total);
        let now = rank(event.phase);
        match last {
            None => assert_eq!(event.current, 0, "the first event starts a phase"),
            Some((was, current)) if now == was => {
                assert_eq!(event.current, current + 1, "one at a time")
            }
            Some((was, _)) => {
                assert!(now > was, "a phase moved backwards");
                assert_eq!(event.current, 0, "a new phase starts at 0");
            }
        }
        last = Some((now, event.current));
    }
});
