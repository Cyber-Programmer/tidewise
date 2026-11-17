//! Review 5.8 R-03 (the re-check): a reset that cannot undo what the caller did must say so.
//!
//! One test, its own process. A caller that is not root can only lower a hard limit, never raise it, so here the
//! file-size hard limit is lowered and the reset cannot lift it: `reset_limits` must return an error, and the helper
//! then refuses to start (`cannot_reset_limits`) instead of running pacman under a limit that can cut a transaction
//! short. A real root (CAP_SYS_RESOURCE, which is what the helper is after pkexec) raises it again and succeeds.

use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
use tidewise_helper::env::reset_limits;

#[test]
fn a_hard_limit_that_cannot_be_raised_is_an_error_not_a_silent_success() {
    setrlimit(
        Resource::Fsize,
        Rlimit {
            current: Some(1024 * 1024),
            maximum: Some(1024 * 1024),
        },
    )
    .unwrap();

    let result = reset_limits();
    if rustix::process::geteuid().is_root() {
        // Only a process with the capability gets here as root: the limit is lifted.
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(getrlimit(Resource::Fsize).current, None);
    } else {
        assert!(
            result.is_err(),
            "the limit stayed at 1 MiB and nobody said so"
        );
    }
}
