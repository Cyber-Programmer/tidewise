//! Live mode: one `LiveStats` sample per second turned into the same `PerfData` the demo uses.

use std::collections::VecDeque;

use tidewise_core::stats::{LiveStats, Snapshot};

use std::collections::HashMap;
use std::path::Path;

use crate::perf::{Fact, Metric, PerfData, TopRow};
use crate::procs::{self, Group, Kind, ProcData, Row};

const HISTORY: usize = 60;
const KB: f64 = 1_000.0;
const MB: f64 = 1_000_000.0;
const GB: f64 = 1_000_000_000.0;

pub struct Sampler {
    stats: LiveStats,
    cpu: VecDeque<f64>,
    memory: VecDeque<f64>,
    network: VecDeque<f64>,
    disk: VecDeque<f64>,
    last: Option<Snapshot>,
}

fn push(series: &mut VecDeque<f64>, value: f64) {
    if series.len() == HISTORY {
        series.pop_front();
    }
    series.push_back(value);
}

/// Top of the chart for a rate series: the biggest value seen, but never below `floor`.
fn rate_scale(series: &VecDeque<f64>, floor: f64) -> f64 {
    series.iter().cloned().fold(floor, f64::max)
}

fn scaled(series: &VecDeque<f64>, scale: f64) -> Vec<f64> {
    series.iter().map(|v| v / scale * 100.0).collect()
}

pub fn fmt_rate(bytes_per_sec: f64) -> (String, &'static str) {
    if bytes_per_sec >= MB {
        (format!("{:.1}", bytes_per_sec / MB), "MB/s")
    } else {
        (format!("{:.0}", bytes_per_sec / KB), "KB/s")
    }
}

pub fn fmt_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

pub fn mood(fraction: f64) -> &'static str {
    match fraction {
        f if f < 0.25 => "Idle",
        f if f < 0.7 => "Busy",
        _ => "Very busy",
    }
}

fn fact(k: &str, v: String) -> Fact {
    Fact { k: k.into(), v }
}

impl Sampler {
    pub fn new() -> Self {
        Sampler {
            stats: LiveStats::new(),
            cpu: VecDeque::new(),
            memory: VecDeque::new(),
            network: VecDeque::new(),
            disk: VecDeque::new(),
            last: None,
        }
    }

    pub fn sample(&mut self) -> PerfData {
        let s = self.stats.sample();
        let total = (s.memory_total as f64).max(1.0);
        push(&mut self.cpu, s.cpu_percent as f64);
        push(&mut self.memory, s.memory_used as f64 / total * 100.0);
        push(&mut self.network, s.net_down_per_sec.unwrap_or(0.0));
        push(
            &mut self.disk,
            s.disk_read_per_sec.unwrap_or(0.0) + s.disk_write_per_sec.unwrap_or(0.0),
        );
        let data = self.build(&s);
        self.last = Some(s);
        data
    }

    /// Rows for the Processes screen from the last sample, grouped by app.
    pub fn processes(&self, desktop: &HashMap<String, String>) -> ProcData {
        let procs: Vec<procs::Proc> = self
            .last
            .iter()
            .flat_map(|s| s.processes.iter())
            .map(|p| {
                let (comm, cmdline) = procs::read_proc(Path::new("/proc"), p.pid)
                    .unwrap_or_else(|| (p.name.clone(), Vec::new()));
                procs::Proc {
                    pid: p.pid,
                    comm,
                    cmdline,
                    cpu: p.cpu_percent as f64,
                    mem: p.memory_bytes,
                }
            })
            .collect();
        let rows = procs::group(&procs, desktop)
            .iter()
            .map(row_from_group)
            .collect();
        ProcData {
            subtitle: format!(
                "{} running, grouped by the app they belong to.",
                procs.len()
            ),
            rows,
        }
    }

    fn build(&self, s: &Snapshot) -> PerfData {
        let total = (s.memory_total as f64).max(1.0);
        let net_now = self.network.back().copied().unwrap_or(0.0);
        let disk_now = self.disk.back().copied().unwrap_or(0.0);
        let net_scale = rate_scale(&self.network, MB);
        let disk_scale = rate_scale(&self.disk, MB);
        let (nv, nu) = fmt_rate(net_now);
        let (dv, du) = fmt_rate(disk_now);
        let (up_v, up_u) = fmt_rate(s.net_up_per_sec.unwrap_or(0.0));
        let (rd_v, rd_u) = fmt_rate(s.disk_read_per_sec.unwrap_or(0.0));
        let (wr_v, wr_u) = fmt_rate(s.disk_write_per_sec.unwrap_or(0.0));
        let top_by = |key: &dyn Fn(&tidewise_core::stats::ProcessSample) -> f64,
                      show: &dyn Fn(f64) -> String| {
            let mut procs: Vec<_> = s.processes.iter().collect();
            procs.sort_by(|a, b| key(b).total_cmp(&key(a)));
            let best = procs
                .first()
                .map(|p| key(p))
                .unwrap_or(0.0)
                .max(f64::MIN_POSITIVE);
            procs
                .into_iter()
                .take(3)
                .map(|p| TopRow {
                    name: p.name.clone(),
                    value: show(key(p)),
                    width: (key(p) / best).clamp(0.0, 1.0),
                })
                .collect::<Vec<_>>()
        };
        let cpu = s.cpu_percent as f64;
        let mem_frac = s.memory_used as f64 / total;
        let metrics = vec![
            Metric {
                label: "Processor".into(),
                value: format!("{cpu:.0}"),
                unit: "%".into(),
                mood: mood(cpu / 100.0).into(),
                axis: "100%".into(),
                headline: "How busy your processor is".into(),
                explain: "Short spikes are apps opening or pages loading, which is normal. If it sits near the top for minutes, check Processes to see which app is busy.".into(),
                vals: scaled(&self.cpu, 100.0),
                facts: vec![
                    fact("Busy now", format!("{cpu:.0}%")),
                    fact("Running programs", s.processes.len().to_string()),
                ],
                top: top_by(&|p| p.cpu_percent as f64, &|v| format!("{v:.1}%")),
            },
            Metric {
                label: "Memory".into(),
                value: format!("{:.1}", s.memory_used as f64 / GB),
                unit: format!("of {:.0} GB", total / GB),
                mood: mood(mem_frac).into(),
                axis: format!("{:.0} GB", total / GB),
                headline: "How much memory apps use".into(),
                explain: "Linux also borrows spare memory to make things faster and hands it back the moment an app needs it, so a high number alone is not a problem.".into(),
                vals: self.memory.iter().cloned().collect(),
                facts: vec![
                    fact("In use", fmt_bytes(s.memory_used)),
                    fact("Installed", fmt_bytes(s.memory_total)),
                    fact("Not in use", fmt_bytes(s.memory_total.saturating_sub(s.memory_used))),
                ],
                top: top_by(&|p| p.memory_bytes as f64, &|v| fmt_bytes(v as u64)),
            },
            Metric {
                label: "Network".into(),
                value: nv,
                unit: format!("{nu} down"),
                mood: if net_now > MB { "Downloading" } else { "Quiet" }.into(),
                axis: {
                    let (v, u) = fmt_rate(net_scale);
                    format!("{v} {u}")
                },
                headline: "Data coming in".into(),
                explain: "If the internet feels slow in other apps, a big download elsewhere may be using the connection.".into(),
                vals: scaled(&self.network, net_scale),
                facts: vec![fact("Upload now", format!("{up_v} {up_u}"))],
                top: Vec::new(),
            },
            Metric {
                label: "Disk".into(),
                value: dv,
                unit: du.into(),
                mood: if disk_now > 20.0 * MB { "Busy" } else { "Quiet" }.into(),
                axis: {
                    let (v, u) = fmt_rate(disk_scale);
                    format!("{v} {u}")
                },
                headline: "Reading and writing".into(),
                explain: "If this stays near the top for a long time, apps will open slowly. Usually that means a large copy, an update or the file indexer.".into(),
                vals: scaled(&self.disk, disk_scale),
                facts: vec![
                    fact("Read now", format!("{rd_v} {rd_u}")),
                    fact("Write now", format!("{wr_v} {wr_u}")),
                ],
                top: Vec::new(),
            },
        ];
        PerfData {
            subtitle: "Live · last 60 seconds.".into(),
            metrics,
            threads: Vec::new(),
        }
    }
}

const AVATARS: [(u8, u8, u8); 8] = [
    (0xFF, 0xB3, 0x8A),
    (0x7A, 0xA7, 0xFF),
    (0x9F, 0xC1, 0xFF),
    (0x8B, 0xEA, 0xCB),
    (0x7F, 0xB2, 0xE5),
    (0xC3, 0xA2, 0xFF),
    (0xB4, 0xBA, 0xC5),
    (0xF2, 0xB8, 0x4B),
];

fn avatar_color(name: &str) -> (u8, u8, u8) {
    let sum: usize = name.bytes().map(usize::from).sum();
    AVATARS[sum % AVATARS.len()]
}

pub fn row_from_group(g: &Group) -> Row {
    let kind = match g.kind {
        Kind::App => "App",
        Kind::Background => "Background service",
        Kind::System => "System",
    };
    let count = if g.count == 1 {
        "1 process".to_string()
    } else {
        format!("{} processes", g.count)
    };
    let note = match g.kind {
        Kind::App => "Ending it closes its windows. Unsaved work in it may be lost.",
        Kind::Background => "Runs in the background. Ending it is usually safe, but it may start again.",
        Kind::System => "Part of your system. Ending it could close your session or cut sound or network, so Tidewise won\u{2019}t.",
    };
    let children = if g.count > 1 {
        format!("  ·  {} child processes", g.count - 1)
    } else {
        String::new()
    };
    Row {
        id: g.key.clone(),
        name: g.name.clone(),
        kind: g.kind,
        sub: format!("{kind} · {count}"),
        sub_details: format!("{kind} · PID {}", g.pid),
        color: avatar_color(&g.name),
        protected: g.protected,
        cpu: format!("{:.1}%", g.cpu),
        cpu_hot: g.cpu >= 5.0,
        mem: fmt_bytes(g.mem),
        mem_hot: g.mem as f64 >= GB,
        disk: "–".into(),
        net: "–".into(),
        note: note.into(),
        cmd: format!("{}{children}", g.cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_scale_never_drops_below_the_floor() {
        let q: VecDeque<f64> = [10.0, 20.0].into();
        assert_eq!(rate_scale(&q, MB), MB);
        let q: VecDeque<f64> = [5.0 * MB].into();
        assert_eq!(rate_scale(&q, MB), 5.0 * MB);
    }

    #[test]
    fn rates_switch_unit_at_one_megabyte() {
        assert_eq!(fmt_rate(999_000.0), ("999".to_string(), "KB/s"));
        assert_eq!(fmt_rate(2_400_000.0), ("2.4".to_string(), "MB/s"));
    }

    #[test]
    fn moods_follow_the_design_words() {
        assert_eq!(mood(0.14), "Idle");
        assert_eq!(mood(0.6), "Busy");
        assert_eq!(mood(0.9), "Very busy");
    }

    #[test]
    fn scaled_series_is_a_percentage_of_the_scale() {
        let q: VecDeque<f64> = [MB / 2.0].into();
        assert_eq!(scaled(&q, MB), vec![50.0]);
    }

    fn group(count: usize, kind: Kind, mem: u64, cpu: f64) -> Group {
        Group {
            key: "k".into(),
            name: "Firefox".into(),
            kind,
            count,
            cpu,
            mem,
            protected: kind == Kind::System,
            pid: 42,
            cmd: "/usr/bin/firefox".into(),
        }
    }

    #[test]
    fn a_group_becomes_a_row_with_plain_words() {
        let r = row_from_group(&group(14, Kind::App, 2_100_000_000, 6.2));
        assert_eq!(r.sub, "App · 14 processes");
        assert_eq!(r.sub_details, "App · PID 42");
        assert_eq!((r.cpu.as_str(), r.mem.as_str()), ("6.2%", "2.1 GB"));
        assert!(r.cpu_hot && r.mem_hot);
        assert!(r.cmd.ends_with("13 child processes"));
    }

    #[test]
    fn one_process_is_singular_and_cold() {
        let r = row_from_group(&group(1, Kind::Background, 120_000_000, 0.3));
        assert_eq!(r.sub, "Background service · 1 process");
        assert!(!r.cpu_hot && !r.mem_hot);
        assert_eq!(r.cmd, "/usr/bin/firefox");
    }

    #[test]
    fn system_rows_are_protected_and_say_why() {
        let r = row_from_group(&group(3, Kind::System, 1, 0.0));
        assert!(r.protected);
        assert!(r.note.contains("Tidewise won\u{2019}t"));
    }
}
