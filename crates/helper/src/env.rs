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

use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};

/// Sets the umask to 022 and returns the previous one.
pub fn set_umask_022() -> u32 {
    rustix::process::umask(rustix::fs::Mode::from_bits_truncate(0o022)).bits()
}

/// One resource limit of the plan: the soft and the hard value (`None` is unlimited).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    pub resource: Resource,
    pub soft: Option<u64>,
    pub hard: Option<u64>,
}

/// The resource limits the helper sets at start. Review 5.8 R-03.
///
/// `pkexec` passes the caller's limits on, and they apply to root too: a low file-size or CPU limit kills pacman in
/// the middle of unpacking a package, and the caller needs no privilege to set them. So the helper sets its own, and
/// pacman, its scriptlets and its hooks inherit them. Cgroup limits (a memory cap, a CPU quota) belong to the caller's
/// service or scope and cannot be reset this way: documented, not defended. The OOM score is handled separately, below
/// (`reset_oom_score`).
///
/// - file size, CPU time, address space, data segment: unlimited, because a transaction is as big as it is;
/// - core dumps: 0, so a crash of a root process never writes memory to disk;
/// - open files: 1024 soft and 524288 hard, what systemd gives a service. A soft limit of a million would make every
///   program that closes all descriptors up to the limit (shells, install scripts) slow;
/// - stack: 8 MiB soft, the kernel's own default, with the hard limit left open as on a stock system.
pub fn limit_plan() -> Vec<Limit> {
    let limit = |resource, soft, hard| Limit {
        resource,
        soft,
        hard,
    };
    vec![
        limit(Resource::Fsize, None, None),
        limit(Resource::Cpu, None, None),
        limit(Resource::As, None, None),
        limit(Resource::Data, None, None),
        limit(Resource::Core, Some(0), Some(0)),
        limit(Resource::Nofile, Some(1024), Some(524_288)),
        limit(Resource::Stack, Some(8 * 1024 * 1024), None),
    ]
}

/// True when a limit that the plan leaves unlimited is still limited after the reset: the helper could not undo what
/// the caller did (raising a hard limit was refused), so a transaction could still be cut short.
pub fn is_short(planned_soft: Option<u64>, after_soft: Option<u64>) -> bool {
    planned_soft.is_none() && after_soft.is_some()
}

/// Applies `limit_plan`. Where raising the hard limit is not allowed (a process that is not really root, such as the
/// rootless test container), the soft limit is raised as far as the hard one allows. A limit the plan leaves unlimited
/// that still is not unlimited afterwards is an error (`is_short`): the caller's limit is still in force.
pub fn reset_limits() -> std::io::Result<()> {
    for limit in limit_plan() {
        let wanted = Rlimit {
            current: limit.soft,
            maximum: limit.hard,
        };
        if setrlimit(limit.resource, wanted).is_ok() {
            continue;
        }
        let hard = getrlimit(limit.resource).maximum;
        let soft = match (limit.soft, hard) {
            (Some(wanted), Some(hard)) => Some(wanted.min(hard)),
            (Some(wanted), None) => Some(wanted),
            (None, hard) => hard,
        };
        setrlimit(
            limit.resource,
            Rlimit {
                current: soft,
                maximum: hard,
            },
        )?;
        if is_short(limit.soft, getrlimit(limit.resource).current) {
            return Err(std::io::Error::other(
                "a resource limit of the caller could not be lifted",
            ));
        }
    }
    Ok(())
}

/// The helper's own out-of-memory score adjustment (`oom_score_adj`, -1000 to 1000; the kernel kills the process with
/// the highest score first). Review 5.8 R-03 re-check, question 37.
pub const OOM_SCORE_ADJ_PATH: &str = "/proc/self/oom_score_adj";

/// The highest `oom_score_adj` the helper starts with: 200. Two different numbers are easy to mix up here:
///
/// - **200 is the value session processes run at.** The user manager's `DefaultOOMScoreAdjust` is its own score plus 100
///   (systemd-system.conf(5), systemd 262.2), and the manager itself runs at 100 because PID 1 starts it from
///   `user@.service` with `OOMScoreAdjust=100` (`/usr/lib/systemd/system/user@.service:35`). Rootless Podman gives 200
///   too. Measured on 17 and 18 Nov 2026.
/// - **100 is the floor an ordinary process can lower itself to.** The kernel says the value "may be reduced no lower than
///   the last value set by a CAP_SYS_RESOURCE process" (`Documentation/filesystems/proc.rst`, `/proc/<pid>/oom_score_adj`;
///   page of kernel 7.4.0-rc3). PID 1 set 100 for the user manager, so on this host `choom -n 100` works and
///   `choom -n 99` fails with "Permission denied".
///
/// Anything above 200 was raised on purpose (a caller can raise its own score to 1000 and have the OOM killer pick root
/// pacman first, in the middle of unpacking), so the helper refuses it. A real root behind pkexec always reaches 0 (it
/// has CAP_SYS_RESOURCE), so on a real system the limit never triggers. It only matters where root lacks the host's
/// capability: containers and user namespaces, which cannot lower the value they were given.
pub const MAX_ACCEPTED_OOM_SCORE_ADJ: i32 = 200;

/// The number in the text of `/proc/self/oom_score_adj`: an optional minus sign and digits, in the kernel's range
/// -1000 to 1000, after one trailing new line. Anything else is `None`.
pub fn parse_oom_score_adj(text: &str) -> Option<i32> {
    let number = text.strip_suffix('\n').unwrap_or(text);
    let digits = number.strip_prefix('-').unwrap_or(number);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value: i32 = number.parse().ok()?;
    (-1000..=1000).contains(&value).then_some(value)
}

/// True when the value in `text` is one the helper accepts: parseable and at most `MAX_ACCEPTED_OOM_SCORE_ADJ`.
pub fn oom_score_is_acceptable(text: &str) -> bool {
    parse_oom_score_adj(text).is_some_and(|value| value <= MAX_ACCEPTED_OOM_SCORE_ADJ)
}

/// Writes `0` to `OOM_SCORE_ADJ_PATH` (never a negative value: protecting pacman over the desktop could freeze the
/// machine), reads the value back and judges it. A failed write is not an error by itself (a process that may not
/// lower the value keeps the one it has); what counts is what is there afterwards. A missing or unparseable file is
/// an error: unknown is not fine.
pub fn reset_oom_score() -> std::io::Result<()> {
    let _ = std::fs::write(OOM_SCORE_ADJ_PATH, "0");
    let text = std::fs::read_to_string(OOM_SCORE_ADJ_PATH)?;
    if oom_score_is_acceptable(&text) {
        Ok(())
    } else {
        Err(std::io::Error::other(
            "the out-of-memory score of the caller could not be lowered",
        ))
    }
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

    #[test]
    fn the_helper_sets_exactly_these_limits() {
        use Resource::{As, Core, Cpu, Data, Fsize, Nofile, Stack};
        let limit = |resource, soft, hard| Limit {
            resource,
            soft,
            hard,
        };
        assert_eq!(
            limit_plan(),
            vec![
                limit(Fsize, None, None),
                limit(Cpu, None, None),
                limit(As, None, None),
                limit(Data, None, None),
                limit(Core, Some(0), Some(0)),
                // What systemd gives a service: 1024 soft (a program that closes every descriptor up to the limit
                // stays fast), 524288 hard.
                limit(Nofile, Some(1024), Some(524_288)),
                // The kernel's default stack, with the hard limit left open as it is everywhere else.
                limit(Stack, Some(8 * 1024 * 1024), None),
            ]
        );
    }

    #[test]
    fn a_limit_that_should_be_unlimited_but_is_not_is_short_of_the_plan() {
        assert!(is_short(None, Some(1_048_576)));
        assert!(is_short(None, Some(0)));
        assert!(!is_short(None, None));
        // A bounded plan is not judged here (a lower value than planned only makes a transaction cut short by the
        // caller for those three, and none of them is one pacman can run into).
        assert!(!is_short(Some(1024), Some(256)));
        assert!(!is_short(Some(0), Some(0)));
    }

    #[test]
    fn the_accepted_oom_score_is_pinned_at_what_the_desktop_and_rootless_podman_give() {
        assert_eq!(MAX_ACCEPTED_OOM_SCORE_ADJ, 200);
    }

    #[test]
    fn the_oom_score_text_is_parsed_strictly() {
        for (text, number) in [
            ("0\n", 0),
            ("0", 0),
            ("200\n", 200),
            ("1000\n", 1000),
            ("-1000\n", -1000),
            ("-1\n", -1),
        ] {
            assert_eq!(parse_oom_score_adj(text), Some(number), "{text:?}");
        }
        for bad in [
            "",
            "\n",
            "abc\n",
            "1 2\n",
            "+5\n",
            "0x10\n",
            "1001\n",
            "-1001\n",
            "99999999999\n",
            "1.5\n",
            "--1\n",
            "- 1\n",
            "5\n\n",
            " 5\n",
        ] {
            assert_eq!(parse_oom_score_adj(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn only_a_score_up_to_the_accepted_one_is_acceptable() {
        for fine in ["0\n", "100\n", "200\n", "-500\n", "-1000\n"] {
            assert!(oom_score_is_acceptable(fine), "{fine:?}");
        }
        for not_fine in ["201\n", "500\n", "1000\n", "", "abc\n", "1001\n", "\n"] {
            assert!(!oom_score_is_acceptable(not_fine), "{not_fine:?}");
        }
    }
}
