//! Live mode: one `LiveStats` sample per second turned into the same `PerfData` the demo uses.

use std::collections::VecDeque;

use tidewise_core::stats::{LiveStats, Snapshot};

use crate::perf::{Fact, Metric, PerfData, TopRow};

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
        self.build(&s)
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
}
