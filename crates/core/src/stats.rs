//! Live system stats. Call [`LiveStats::sample`] about once per second.

use std::fs;
use std::time::Instant;

use sysinfo::{Networks, ProcessesToUpdate, System};

use crate::diskstats::{parse_diskstats, DiskCounters};
use crate::rates::rate_per_sec;
use crate::statm::{page_size_from, parse_statm};

/// One process as shown in the process list.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessSample {
    pub pid: u32,
    pub name: String,
    /// Share of the whole PC, 0 to 100 (the per-core value divided by the logical CPU count).
    pub cpu_percent: f32,
    /// Percent of one core, so it can go above 100 for busy multi-threaded programs.
    /// Shown in technical details as "x % of one core".
    pub cpu_core_percent: f32,
    /// Private memory: (resident − shared) pages. Safe to add up across a group of processes.
    pub memory_bytes: u64,
    /// Resident set size, including shared libraries. For technical details of a single process.
    pub rss_bytes: u64,
}

/// Per-core percent (0 to 100 × cores) as a share of the whole machine, 0 to 100.
pub fn cpu_share(core_percent: f32, logical_cpus: usize) -> f32 {
    (core_percent / logical_cpus.max(1) as f32).clamp(0.0, 100.0)
}

/// Everything the Performance screen needs from one sampling.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Whole-machine CPU use, 0 to 100.
    pub cpu_percent: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    /// Bytes per second. `None` on the first sample, when there is nothing to compare with.
    pub disk_read_per_sec: Option<f64>,
    pub disk_write_per_sec: Option<f64>,
    pub net_down_per_sec: Option<f64>,
    pub net_up_per_sec: Option<f64>,
    pub processes: Vec<ProcessSample>,
}

#[derive(Debug, Clone, Copy)]
struct Counters {
    at: Instant,
    disk: DiskCounters,
    net_down: u64,
    net_up: u64,
}

pub struct LiveStats {
    system: System,
    networks: Networks,
    previous: Option<Counters>,
}

impl Default for LiveStats {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveStats {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            networks: Networks::new_with_refreshed_list(),
            previous: None,
        }
    }

    /// Take one sample. Rates compare against the sample from the previous call.
    pub fn sample(&mut self) -> Snapshot {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.networks.refresh(true);

        let now = Counters {
            at: Instant::now(),
            disk: read_disk_counters(),
            net_down: self.networks.values().map(|n| n.total_received()).sum(),
            net_up: self.networks.values().map(|n| n.total_transmitted()).sum(),
        };
        let cpus = self.system.cpus().len();
        let prev = self.previous.replace(now);
        let interval = prev.map(|p| now.at.duration_since(p.at));
        let rate =
            |before: Option<u64>, after: u64| interval.and_then(|i| rate_per_sec(before, after, i));

        Snapshot {
            cpu_percent: self.system.global_cpu_usage().clamp(0.0, 100.0),
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            disk_read_per_sec: rate(prev.map(|p| p.disk.read_bytes), now.disk.read_bytes),
            disk_write_per_sec: rate(prev.map(|p| p.disk.write_bytes), now.disk.write_bytes),
            net_down_per_sec: rate(prev.map(|p| p.net_down), now.net_down),
            net_up_per_sec: rate(prev.map(|p| p.net_up), now.net_up),
            processes: self
                .system
                .processes()
                .values()
                // On Linux sysinfo also lists threads, each with its parent's whole memory.
                .filter(|p| p.thread_kind().is_none())
                .map(|p| {
                    let pid = p.pid().as_u32();
                    let rss = p.memory();
                    let private = fs::read_to_string(format!("/proc/{pid}/statm"))
                        .ok()
                        .and_then(|t| parse_statm(&t))
                        .map(|m| m.private_bytes(page_size_from(rss, &m)))
                        .unwrap_or(rss);
                    ProcessSample {
                        pid,
                        name: p.name().to_string_lossy().into_owned(),
                        cpu_percent: cpu_share(p.cpu_usage(), cpus),
                        cpu_core_percent: p.cpu_usage(),
                        memory_bytes: private,
                        rss_bytes: rss,
                    }
                })
                .collect(),
        }
    }
}

/// A device counts as a whole disk when `/sys/block/<name>` exists (partitions live below it).
/// Loop and RAM disks are left out.
fn is_whole_disk(name: &str) -> bool {
    !name.starts_with("loop")
        && !name.starts_with("ram")
        && fs::metadata(format!("/sys/block/{name}")).is_ok()
}

fn read_disk_counters() -> DiskCounters {
    fs::read_to_string("/proc/diskstats")
        .map(|text| parse_diskstats(&text, is_whole_disk))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::cpu_share;

    #[test]
    fn busy_on_all_cores_is_one_hundred_percent_of_the_pc() {
        assert_eq!(cpu_share(1200.0, 12), 100.0);
        assert_eq!(cpu_share(100.0, 12), 100.0 / 12.0);
    }

    #[test]
    fn share_never_leaves_zero_to_one_hundred() {
        assert_eq!(cpu_share(5000.0, 12), 100.0);
        assert_eq!(cpu_share(-1.0, 12), 0.0);
        assert_eq!(
            cpu_share(50.0, 0),
            50.0,
            "zero cpus must not divide by zero"
        );
    }
}
