//! Review 5.8 R-03: the limits the caller set are reset when the helper starts (threat T-08).
//!
//! `reset_limits` changes limits of the whole process, so this file has one test and its own process. The test
//! lowers the soft limits the way a caller could (it needs no privilege), calls `reset_limits`, and reads the limits
//! back from `/proc/self/limits`. It does not need to be root: where the hard limit forbids raising a limit as far as
//! the plan says, the soft limit must reach the hard one, and the expectation below says so.

use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
use tidewise_helper::env::{limit_plan, reset_limits};

fn lower(resource: Resource, soft: u64) {
    let hard = getrlimit(resource).maximum;
    setrlimit(
        resource,
        Rlimit {
            current: Some(soft),
            maximum: hard,
        },
    )
    .unwrap();
}

/// The smaller of a planned value and the hard limit that was there before (where raising is not allowed, the
/// result cannot be above it); `None` is unlimited.
fn capped(planned: Option<u64>, hard_before: Option<u64>) -> Option<u64> {
    match (planned, hard_before) {
        (Some(wanted), Some(hard)) => Some(wanted.min(hard)),
        (Some(wanted), None) => Some(wanted),
        (None, hard) => hard,
    }
}

#[test]
fn limits_the_caller_lowered_are_reset_to_the_planned_values() {
    // The caller's doing: every limit of the plan pushed to something small (but never so small that the test
    // process itself stops working).
    for resource in [Resource::As, Resource::Data] {
        lower(resource, 4 * 1024 * 1024 * 1024);
    }
    lower(Resource::Fsize, 1024 * 1024);
    lower(Resource::Cpu, 600);
    lower(Resource::Nofile, 256);
    lower(Resource::Stack, 1024 * 1024);
    lower(Resource::Core, 1024 * 1024);

    let hard_before: Vec<_> = limit_plan()
        .iter()
        .map(|limit| getrlimit(limit.resource).maximum)
        .collect();
    reset_limits().unwrap();

    let plan = limit_plan();
    assert_eq!(plan.len(), 7, "the plan names the seven limits");
    for (limit, hard) in plan.into_iter().zip(hard_before) {
        let after = getrlimit(limit.resource);
        let hard_want = capped(limit.hard, hard);
        // The soft limit cannot end above the hard one.
        let soft_want = capped(limit.soft, hard_want);
        assert_eq!(after.current, soft_want, "{:?}: soft limit", limit.resource);
        assert_eq!(after.maximum, hard_want, "{:?}: hard limit", limit.resource);
    }
}
