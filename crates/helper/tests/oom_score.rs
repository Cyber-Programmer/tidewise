//! Question 37 (the re-check of the R-03 fix): the caller's `oom_score_adj` survives pkexec, so a caller that raised its
//! own to 1000 would have the OOM killer pick root pacman first, in the middle of unpacking. The helper writes 0 at
//! start and judges what is there afterwards.
//!
//! One test, its own process (it changes the process's own score). Measured on 17 Nov 2026: an ordinary user can
//! lower its own value only down to the minimum it inherited (100 from the systemd user manager on the development
//! machine), not to 0. So the test, which runs as whoever runs `cargo test`, expects what the helper promises: the
//! value ends at or below the accepted one, and a real root (CAP_SYS_RESOURCE, which is what the helper is after
//! pkexec) ends at 0. What it cannot reach is the case the refusal exists for: a root that may not lower the value
//! (rootless Podman gives the container 200 and refuses to go below). That case is covered by the unit tests of the
//! judgement and by the container test through real pkexec.

use std::fs;

use tidewise_helper::env::{MAX_ACCEPTED_OOM_SCORE_ADJ, OOM_SCORE_ADJ_PATH, reset_oom_score};

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
fn a_score_raised_by_the_caller_is_brought_down_or_refused_consistently() {
    let is_root = rustix::process::geteuid().is_root();
    for raised in [100, 200, 1000] {
        // Can this process lower its own score to 0? (A real root can; an ordinary user only down to the minimum it
        // inherited, which is above 0 under a systemd user manager.) The probe lowers it; the next line raises it again.
        let can_reach_zero = fs::write(OOM_SCORE_ADJ_PATH, "0").is_ok();
        set_own_score(raised);

        let result = reset_oom_score();
        let now = own_score();
        // The verdict always agrees with the value that is left.
        assert_eq!(
            result.is_ok(),
            now <= MAX_ACCEPTED_OOM_SCORE_ADJ,
            "{raised} became {now}: {result:?}"
        );
        // Where zero is reachable the helper must have reached it.
        if is_root || can_reach_zero {
            assert!(result.is_ok(), "{raised}: {result:?}");
            assert_eq!(now, 0, "zero was reachable, from {raised}");
        }
    }
}
