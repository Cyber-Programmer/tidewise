//! Question 37 (the re-check of the R-03 fix): the caller's `oom_score_adj` survives pkexec, so a caller that raised its
//! own to 1000 would have the OOM killer pick root pacman first, in the middle of unpacking. The helper writes 0 at
//! start and judges what is there afterwards.
//!
//! One test, its own process (it changes the process's own score). Measured on 17 Nov 2026: on the host even an
//! ordinary user can lower its own value down to its minimum (0 here), so this test, which runs as whoever runs
//! `cargo test`, ends at 0 each time. What it cannot reach is the case the refusal exists for: a root that may not
//! lower the value (rootless Podman gives the container 200 and refuses to go below). That case is covered by the unit
//! tests of the judgement and by the container test through real pkexec.

use std::fs;

use tidewise_helper::env::{OOM_SCORE_ADJ_PATH, reset_oom_score};

fn set_own_score(value: i32) {
    fs::write(OOM_SCORE_ADJ_PATH, format!("{value}\n")).unwrap();
}

fn own_score() -> i32 {
    fs::read_to_string(OOM_SCORE_ADJ_PATH)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn a_score_raised_by_the_caller_is_brought_back_to_zero() {
    for raised in [100, 200, 1000] {
        set_own_score(raised);
        let result = reset_oom_score();
        assert!(result.is_ok(), "{raised}: {result:?}");
        assert_eq!(own_score(), 0, "{raised}");
    }
}
