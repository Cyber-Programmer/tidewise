//! The process's umask and the environment pacman runs in (threats T-08, T-13).
//!
//! pacman does not fix the umask itself: measured in the test container, a downloaded package file in
//! `/var/cache/pacman/pkg` came out mode 600 under umask 077 and 666 (world-writable) under umask 000. libalpm only
//! sets `umask(0022)` locally, while unpacking and for scriptlets and hooks (`lib/libalpm/util.c`). So the helper
//! sets 022 itself, first thing, before anything is created. `pkexec` passes on the caller's umask.
//!
//! The standard library has no `umask`, and this crate forbids `unsafe`, so the call is `rustix::process::umask`
//! (rustix 1.1.6, `src/process/umask.rs:19`, gated by rustix's `fs` feature), a safe function.
//!
//! The environment of pacman, and so of every install scriptlet and hook it runs, is not empty and not the caller's:
//! a fixed pair. (Measured: a scriptlet gets exactly this plus what pacman itself adds, `HTTP_USER_AGENT`, `PWD`,
//! `SHLVL`. With no `PATH` the shell supplies its own default, so `PATH` is not strictly needed; it is set anyway so
//! the environment is fixed and readable.)

/// Sets the umask to 022 and returns the previous one.
pub fn set_umask_022() -> u32 {
    rustix::process::umask(rustix::fs::Mode::from_bits_truncate(0o022)).bits()
}

/// The only environment variables pacman gets.
pub fn pacman_environment() -> Vec<(&'static str, &'static str)> {
    vec![("PATH", "/usr/bin"), ("LC_ALL", "C")]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `Umask:` line of `/proc/self/status`.
    fn current_umask() -> String {
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        status
            .lines()
            .find_map(|line| line.strip_prefix("Umask:"))
            .unwrap()
            .trim()
            .to_string()
    }

    #[test]
    fn the_umask_becomes_022_whatever_the_caller_had() {
        // Only this test changes the umask of the test process; it ends with 022 as well.
        for before in [0o077, 0o000, 0o027, 0o022] {
            rustix::process::umask(rustix::fs::Mode::from_bits_truncate(before));
            let previous = set_umask_022();
            assert_eq!(previous, before, "the previous mask is returned");
            assert_eq!(current_umask(), "0022", "after starting from {before:o}");
        }
    }

    #[test]
    fn pacman_gets_exactly_path_and_lc_all() {
        assert_eq!(
            pacman_environment(),
            vec![("PATH", "/usr/bin"), ("LC_ALL", "C")]
        );
    }

    #[test]
    fn nothing_of_the_callers_environment_is_in_it() {
        // The list is a constant: it does not read the environment at all, so a hostile one cannot reach it.
        for (key, _) in pacman_environment() {
            assert!(matches!(key, "PATH" | "LC_ALL"), "{key}");
        }
    }
}
