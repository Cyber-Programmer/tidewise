//! Reading processes straight from `/proc/<pid>/{stat,statm}`.
//!
//! sysinfo needs about 200 µs per process even when asked for nothing; two small file reads are
//! far cheaper, and `/proc/<pid>` lists processes only (threads live below `task/`).

use std::fs;
use std::path::Path;

use crate::statm::{parse_statm, Statm};

/// Clock ticks per second in `/proc/<pid>/stat`. Fixed at 100 on Linux for user space (USER_HZ).
pub const USER_HZ: f64 = 100.0;

/// The fields of `/proc/<pid>/stat` we use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub comm: String,
    /// utime + stime, in clock ticks.
    pub ticks: u64,
    /// When the process started, in ticks after boot. Tells a reused pid from the old process.
    pub start_time: u64,
    pub rss_pages: u64,
}

/// Parse one line of `stat`. The name sits between the first `(` and the last `)` and may contain
/// spaces and parentheses, so everything after the last `)` is split by whitespace.
pub fn parse_stat(text: &str) -> Option<Stat> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close < open {
        return None;
    }
    let comm = text[open + 1..close].to_string();
    let rest: Vec<&str> = text[close + 1..].split_whitespace().collect();
    // After the name: state is index 0, utime 11, stime 12, starttime 19, rss 21.
    let num = |i: usize| rest.get(i)?.parse::<u64>().ok();
    Some(Stat {
        comm,
        ticks: num(11)?.saturating_add(num(12)?),
        start_time: num(19)?,
        rss_pages: num(21)?,
    })
}

/// One process as read from `/proc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawProc {
    pub pid: u32,
    pub stat: Stat,
    pub statm: Option<Statm>,
}

/// Every process under `root` (normally `/proc`). Processes that vanish while reading are skipped.
pub fn scan(root: &Path) -> Vec<RawProc> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| {
            let e = e.ok()?;
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let dir = e.path();
            let stat = parse_stat(&fs::read_to_string(dir.join("stat")).ok()?)?;
            let statm = fs::read_to_string(dir.join("statm"))
                .ok()
                .and_then(|t| parse_statm(&t));
            Some(RawProc { pid, stat, statm })
        })
        .collect()
}

/// Percent of one core used since the previous reading `secs` ago; 0 when there is nothing to
/// compare with, the pid was reused, or the counter went backwards.
pub fn core_percent(previous: Option<(u64, u64)>, start_time: u64, ticks: u64, secs: f64) -> f32 {
    match previous {
        Some((prev_start, prev_ticks))
            if prev_start == start_time && ticks >= prev_ticks && secs > 0.0 =>
        {
            ((ticks - prev_ticks) as f64 / USER_HZ / secs * 100.0) as f32
        }
        _ => 0.0,
    }
}

/// Kernel page size in bytes, from the `KernelPageSize:` line of a `smaps` text; 4096 if absent.
pub fn parse_page_size(smaps: &str) -> u64 {
    smaps
        .lines()
        .find_map(|l| {
            let kb = l
                .strip_prefix("KernelPageSize:")?
                .trim()
                .strip_suffix("kB")?;
            kb.trim().parse::<u64>().ok()
        })
        .map_or(4096, |kb| kb * 1024)
}

/// Page size of this machine (read once from our own `smaps`).
pub fn page_size() -> u64 {
    fs::read_to_string("/proc/self/smaps")
        .map(|t| parse_page_size(&t))
        .unwrap_or(4096)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAT: &str = include_str!("../../../tests/fixtures/proc/stat-cat.txt");
    const SPACE: &str = include_str!("../../../tests/fixtures/proc/stat-comm-with-space.txt");
    const KTHREAD: &str = include_str!("../../../tests/fixtures/proc/stat-kernel-thread.txt");
    const BROKEN: &str = include_str!("../../../tests/fixtures/proc/stat-broken.txt");

    #[test]
    fn parses_a_real_stat_line() {
        let s = parse_stat(CAT).unwrap();
        assert_eq!(s.comm, "cat");
        assert_eq!(s.rss_pages, 567);
        assert_eq!(s.start_time, 7_851_761);
        assert_eq!(s.ticks, 0);
    }

    #[test]
    fn a_name_with_spaces_is_kept_whole() {
        let s = parse_stat(SPACE).unwrap();
        assert_eq!(s.comm, "tmux: client");
        assert_eq!(s.rss_pages, 1348);
    }

    #[test]
    fn a_name_with_parentheses_does_not_confuse_the_fields() {
        let s =
            parse_stat("42 (a) b (c)) S 1 1 1 0 -1 0 0 0 0 0 7 5 0 0 20 0 1 0 900 0 33 0").unwrap();
        assert_eq!(s.comm, "a) b (c)");
        assert_eq!((s.ticks, s.start_time, s.rss_pages), (12, 900, 33));
    }

    #[test]
    fn kernel_threads_have_no_memory() {
        let s = parse_stat(KTHREAD).unwrap();
        assert_eq!((s.comm.as_str(), s.rss_pages), ("kthreadd", 0));
    }

    #[test]
    fn broken_or_short_lines_are_none() {
        assert_eq!(parse_stat(BROKEN), None);
        assert_eq!(parse_stat(""), None);
        assert_eq!(parse_stat("1 (x) S 1 2"), None);
        assert_eq!(parse_stat("1 )x( S"), None);
    }

    #[test]
    fn cpu_is_ticks_gained_per_second() {
        // 150 ticks in 2 s at 100 Hz = 75 % of one core.
        assert_eq!(core_percent(Some((9, 1000)), 9, 1150, 2.0), 75.0);
        // Four busy cores can go well above 100 % of one core.
        assert_eq!(core_percent(Some((9, 0)), 9, 400, 1.0), 400.0);
    }

    #[test]
    fn cpu_is_zero_without_a_trustworthy_previous_reading() {
        assert_eq!(core_percent(None, 9, 500, 1.0), 0.0);
        assert_eq!(core_percent(Some((8, 100)), 9, 500, 1.0), 0.0, "reused pid");
        assert_eq!(
            core_percent(Some((9, 600)), 9, 500, 1.0),
            0.0,
            "counter went back"
        );
        assert_eq!(
            core_percent(Some((9, 100)), 9, 500, 0.0),
            0.0,
            "no time passed"
        );
    }

    #[test]
    fn page_size_comes_from_smaps_or_defaults() {
        assert_eq!(
            parse_page_size("Size: 4 kB\nKernelPageSize:       16 kB\n"),
            16 * 1024
        );
        assert_eq!(parse_page_size("nothing useful"), 4096);
        assert_eq!(parse_page_size(""), 4096);
    }

    #[test]
    fn scanning_a_fake_proc_lists_only_numeric_directories() {
        let dir = tempfile::tempdir().unwrap();
        for (pid, line) in [("10", CAT), ("11", SPACE)] {
            let d = dir.path().join(pid);
            fs::create_dir_all(d.join("task")).unwrap();
            fs::write(d.join("stat"), line).unwrap();
            fs::write(d.join("statm"), "100 50 20 0 0 0 0").unwrap();
        }
        fs::create_dir_all(dir.path().join("sys")).unwrap();
        fs::create_dir_all(dir.path().join("12")).unwrap(); // vanished: no stat file
        let mut got = scan(dir.path());
        got.sort_by_key(|p| p.pid);
        assert_eq!(got.iter().map(|p| p.pid).collect::<Vec<_>>(), [10, 11]);
        assert_eq!(got[1].stat.comm, "tmux: client");
        assert_eq!(got[0].statm.unwrap().shared_pages, 20);
    }
}
