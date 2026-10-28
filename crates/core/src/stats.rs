//! Live system stats. Call [`LiveStats::sample`] about once per second.

use std::fs;
use std::time::Instant;

use sysinfo::{Networks, ProcessesToUpdate, System};

use crate::diskstats::{parse_diskstats, DiskCounters};
use crate::rates::rate_per_sec;

/// One process as shown in the process list.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessSample {
    pub pid: u32,
    pub name: String,
    /// Percent of one core, so it can go above 100 for busy multi-threaded programs.
    pub cpu_percent: f32,
    pub memory_bytes: u64,
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
                .map(|p| ProcessSample {
                    pid: p.pid().as_u32(),
                    name: p.name().to_string_lossy().into_owned(),
                    cpu_percent: p.cpu_usage(),
                    memory_bytes: p.memory(),
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
