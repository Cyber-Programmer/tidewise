//! Step 1: which file descriptors did we inherit? (threat T-08, open question 6)
//!
//! `pkexec` (polkit 128, `src/programs/pkexec.c`, `fdwalk_close_on_exec (3)`) marks every descriptor from 3 up as
//! close-on-exec before it starts us, so through `pkexec` only 0, 1 and 2 are open. It does not check that 0, 1 and 2
//! themselves are open; Rust's standard library does that at start-up (it reopens a closed one as `/dev/null`).
//! Closing descriptors ourselves would need `unsafe`, which this crate forbids, so we do the next best thing:
//! look at `/proc/self/fd` and refuse to run if anything else is open. This is a second line of defence, not the first.

use std::io;

/// One open descriptor: its number and where `/proc/self/fd/<n>` points (for example `/dev/null` or `pipe:[123]`).
pub type FdEntry = (u32, String);

/// Why the descriptors are not acceptable. Only a number: nothing from the link text is ever repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdProblem {
    /// 0, 1 or 2 is not open.
    Missing(u32),
    /// A descriptor other than 0, 1, 2 and our own directory listing.
    Unexpected(u32),
}

/// Accepts the standard three descriptors, plus (at most) the one descriptor that reading `/proc/self/fd` itself
/// opened. That one is recognised by its target, `/proc/<our pid>/fd`. Reports the lowest offending number.
pub fn check_fds(entries: &[FdEntry], own_pid: u32) -> Result<(), FdProblem> {
    for standard in 0..=2 {
        if !entries.iter().any(|(fd, _)| *fd == standard) {
            return Err(FdProblem::Missing(standard));
        }
    }

    let listing = format!("/proc/{own_pid}/fd");
    let mut listing_seen = false;
    let mut sorted: Vec<&FdEntry> = entries.iter().collect();
    sorted.sort_by_key(|entry| entry.0);
    for (fd, target) in sorted {
        if *fd <= 2 {
            continue;
        }
        if !listing_seen && *target == listing {
            listing_seen = true;
            continue;
        }
        return Err(FdProblem::Unexpected(*fd));
    }
    Ok(())
}

/// Lists `/proc/self/fd`. A name that is not a number, or a link that cannot be read, is an error (fail closed).
pub fn read_fd_entries() -> io::Result<Vec<FdEntry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir("/proc/self/fd")? {
        let item = item?;
        let fd: u32 = item
            .file_name()
            .to_str()
            .and_then(|name| name.parse().ok())
            .ok_or_else(|| io::Error::other("a name in /proc/self/fd is not a number"))?;
        // A link that cannot be read is an error too: unknown is not "fine".
        let target = std::fs::read_link(item.path())?;
        entries.push((fd, target.to_string_lossy().into_owned()));
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn std_fds() -> Vec<FdEntry> {
        vec![
            (0, "/dev/null".to_string()),
            (1, "pipe:[1]".to_string()),
            (2, "pipe:[2]".to_string()),
        ]
    }

    fn with(extra: &[(u32, &str)]) -> Vec<FdEntry> {
        let mut entries = std_fds();
        entries.extend(extra.iter().map(|(fd, target)| (*fd, target.to_string())));
        entries
    }

    #[test]
    fn only_the_standard_descriptors_are_fine() {
        assert_eq!(check_fds(&std_fds(), 77), Ok(()));
    }

    #[test]
    fn our_own_listing_descriptor_is_fine() {
        assert_eq!(check_fds(&with(&[(3, "/proc/77/fd")]), 77), Ok(()));
    }

    #[test]
    fn an_extra_descriptor_is_refused_by_number() {
        let entries = with(&[(3, "/tmp/secret"), (4, "/proc/77/fd")]);
        assert_eq!(check_fds(&entries, 77), Err(FdProblem::Unexpected(3)));
    }

    #[test]
    fn any_kind_of_extra_descriptor_is_refused() {
        for target in [
            "/etc/shadow",
            "socket:[99]",
            "anon_inode:[eventfd]",
            "/dev/null",
            "",
        ] {
            let entries = with(&[(7, target)]);
            assert_eq!(
                check_fds(&entries, 77),
                Err(FdProblem::Unexpected(7)),
                "{target:?}"
            );
        }
    }

    #[test]
    fn a_listing_descriptor_of_another_process_is_refused() {
        assert_eq!(
            check_fds(&with(&[(3, "/proc/78/fd")]), 77),
            Err(FdProblem::Unexpected(3))
        );
    }

    #[test]
    fn a_target_that_only_looks_like_the_listing_is_refused() {
        for target in [
            "/proc/77/fd (deleted)",
            "/proc/77/fd/",
            "/proc/770/fd",
            "/proc/7/fd",
        ] {
            assert_eq!(
                check_fds(&with(&[(3, target)]), 77),
                Err(FdProblem::Unexpected(3)),
                "{target:?}"
            );
        }
    }

    #[test]
    fn only_one_listing_descriptor_is_allowed() {
        let entries = with(&[(3, "/proc/77/fd"), (4, "/proc/77/fd")]);
        assert_eq!(check_fds(&entries, 77), Err(FdProblem::Unexpected(4)));
    }

    #[test]
    fn a_missing_standard_descriptor_is_refused_lowest_first() {
        let no_1: Vec<FdEntry> = std_fds().into_iter().filter(|e| e.0 != 1).collect();
        assert_eq!(check_fds(&no_1, 77), Err(FdProblem::Missing(1)));
        let only_1: Vec<FdEntry> = std_fds().into_iter().filter(|e| e.0 == 1).collect();
        assert_eq!(check_fds(&only_1, 77), Err(FdProblem::Missing(0)));
        assert_eq!(check_fds(&[], 77), Err(FdProblem::Missing(0)));
    }

    #[test]
    fn the_order_of_the_listing_does_not_matter() {
        let mut entries = with(&[(9, "/tmp/a"), (5, "/tmp/b"), (3, "/proc/77/fd")]);
        entries.reverse();
        assert_eq!(check_fds(&entries, 77), Err(FdProblem::Unexpected(5)));
    }

    #[test]
    fn thousands_of_descriptors_are_refused_at_the_lowest() {
        let extra: Vec<(u32, String)> = (3..10_000).map(|n| (n, "x".to_string())).collect();
        let mut entries = std_fds();
        entries.extend(extra);
        assert_eq!(check_fds(&entries, 77), Err(FdProblem::Unexpected(3)));
    }

    #[test]
    fn the_real_listing_has_the_standard_descriptors_and_its_own_listing_descriptor() {
        let entries = read_fd_entries().unwrap();
        for fd in 0..=2 {
            assert!(entries.iter().any(|e| e.0 == fd), "fd {fd} missing");
        }
        let listing = format!("/proc/{}/fd", std::process::id());
        assert!(
            entries.iter().any(|e| e.0 > 2 && e.1 == listing),
            "the listing's own descriptor was not seen: {entries:?}"
        );
    }
}
