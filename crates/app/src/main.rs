//! Slint spike: the Performance screen fed by `tidewise-core`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use slint::{Color, ModelRc, SharedString, Timer, TimerMode, VecModel};
use tidewise_core::stats::{LiveStats, Snapshot};

mod cli;
mod demo;
mod perf;
mod screenshot;

slint::include_modules!();

const HISTORY: usize = 60;
const MAX_ROWS: usize = 200;
const MB: f64 = 1_000_000.0;
const GB: f64 = 1_000_000_000.0;

/// Raw values of the last 60 samples for each of the four metrics.
#[derive(Default)]
struct History {
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

/// SVG path commands for a line through `values` (each 0..=1), right-aligned so a short
/// history grows in from the right edge. Empty input gives an empty path.
fn line_path(values: &[f64], width: f64, height: f64, pad: f64) -> String {
    let n = values.len();
    let mut out = String::new();
    for (i, v) in values.iter().enumerate() {
        let x = (HISTORY - n + i) as f64 * width / (HISTORY - 1) as f64;
        let y = height - pad - v.clamp(0.0, 1.0) * (height - pad * 2.0);
        out.push_str(if i == 0 { "M" } else { " L" });
        out.push_str(&format!("{x:.1} {y:.1}"));
    }
    out
}

/// The same line closed down to the baseline, for the soft fill under the chart.
fn area_path(values: &[f64], width: f64, height: f64, pad: f64) -> String {
    if values.is_empty() {
        return String::new();
    }
    let x0 = (HISTORY - values.len()) as f64 * width / (HISTORY - 1) as f64;
    let base = height - pad;
    format!(
        "M{x0:.1} {base:.1} L{} L{width:.1} {base:.1} Z",
        line_path(values, width, height, pad).trim_start_matches('M')
    )
}

fn normalised(series: &VecDeque<f64>, scale: f64) -> Vec<f64> {
    series.iter().map(|v| v / scale).collect()
}

/// Top of the chart for a rate series: the biggest value seen, but never below `floor`.
fn rate_scale(series: &VecDeque<f64>, floor: f64) -> f64 {
    series.iter().cloned().fold(floor, f64::max)
}

fn fmt_rate(bytes_per_sec: f64) -> (String, &'static str) {
    if bytes_per_sec >= MB {
        (format!("{:.1}", bytes_per_sec / MB), "MB/s")
    } else {
        (format!("{:.0}", bytes_per_sec / 1000.0), "KB/s")
    }
}

fn fmt_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

fn mood(fraction: f64) -> &'static str {
    match fraction {
        f if f < 0.25 => "Calm",
        f if f < 0.7 => "Busy",
        _ => "Very busy",
    }
}

struct App {
    stats: LiveStats,
    history: History,
    total_memory: f64,
    rows: Rc<VecModel<ProcessRow>>,
    snapshot: Option<Snapshot>,
}

impl App {
    fn sample(&mut self) {
        let s = self.stats.sample();
        push(&mut self.history.cpu, s.cpu_percent as f64);
        push(&mut self.history.memory, s.memory_used as f64);
        push(&mut self.history.network, s.net_down_per_sec.unwrap_or(0.0));
        push(
            &mut self.history.disk,
            s.disk_read_per_sec.unwrap_or(0.0) + s.disk_write_per_sec.unwrap_or(0.0),
        );
        self.total_memory = s.memory_total as f64;
        self.snapshot = Some(s);
    }

    fn render(&self, ui: &MainWindow, selected: usize) {
        let Some(s) = &self.snapshot else { return };
        let h = &self.history;
        let mem_scale = self.total_memory.max(1.0);
        let net_scale = rate_scale(&h.network, MB);
        let disk_scale = rate_scale(&h.disk, MB);

        let last = |q: &VecDeque<f64>| q.back().copied().unwrap_or(0.0);
        let (net_v, net_u) = fmt_rate(last(&h.network));
        let (disk_v, disk_u) = fmt_rate(last(&h.disk));

        struct Metric {
            label: &'static str,
            value: String,
            unit: String,
            mood: &'static str,
            headline: &'static str,
            color: Color,
            axis: String,
            values: Vec<f64>,
        }
        let metrics = [
            Metric {
                label: "Processor",
                value: format!("{:.0}", s.cpu_percent),
                unit: "%".into(),
                mood: mood(s.cpu_percent as f64 / 100.0),
                headline: "How busy your processor is",
                color: Color::from_rgb_u8(0x7A, 0xA7, 0xFF),
                axis: "100%".into(),
                values: normalised(&h.cpu, 100.0),
            },
            Metric {
                label: "Memory",
                value: format!("{:.1}", s.memory_used as f64 / GB),
                unit: format!("of {:.0} GB", mem_scale / GB),
                mood: mood(s.memory_used as f64 / mem_scale),
                headline: "How much memory apps use",
                color: Color::from_rgb_u8(0xF2, 0xB8, 0x4B),
                axis: format!("{:.0} GB", mem_scale / GB),
                values: normalised(&h.memory, mem_scale),
            },
            Metric {
                label: "Network",
                value: net_v,
                unit: format!("{net_u} down"),
                mood: if last(&h.network) > MB {
                    "Downloading"
                } else {
                    "Quiet"
                },
                headline: "Data coming in",
                color: Color::from_rgb_u8(0x5D, 0xDC, 0xB0),
                axis: {
                    let (v, u) = fmt_rate(net_scale);
                    format!("{v} {u}")
                },
                values: normalised(&h.network, net_scale),
            },
            Metric {
                label: "Disk",
                value: disk_v,
                unit: disk_u.into(),
                mood: if last(&h.disk) > 20.0 * MB {
                    "Busy"
                } else {
                    "Quiet"
                },
                headline: "Reading and writing",
                color: Color::from_rgb_u8(0xC3, 0xA2, 0xFF),
                axis: {
                    let (v, u) = fmt_rate(disk_scale);
                    format!("{v} {u}")
                },
                values: normalised(&h.disk, disk_scale),
            },
        ];

        let cards: Vec<MetricCard> = metrics
            .iter()
            .map(|m| {
                let tail = &m.values[m.values.len().saturating_sub(30)..];
                let spark_values: Vec<f64> = tail.to_vec();
                // The sparkline shows the last 30 samples across its whole width.
                let n = spark_values.len();
                let mut spark = String::new();
                for (i, v) in spark_values.iter().enumerate() {
                    let x = i as f64 * 120.0 / (n.max(2) - 1) as f64;
                    let y = 32.0 - 2.0 - v.clamp(0.0, 1.0) * 28.0;
                    spark.push_str(if i == 0 { "M" } else { " L" });
                    spark.push_str(&format!("{x:.1} {y:.1}"));
                }
                MetricCard {
                    label: m.label.into(),
                    value: m.value.as_str().into(),
                    unit: m.unit.as_str().into(),
                    mood: m.mood.into(),
                    color: m.color,
                    spark: spark.into(),
                }
            })
            .collect();
        ui.set_cards(ModelRc::new(VecModel::from(cards)));

        let cur = &metrics[selected.min(3)];
        ui.set_selected(selected as i32);
        ui.set_headline(cur.headline.into());
        ui.set_big_value(cur.value.as_str().into());
        ui.set_big_unit(cur.unit.as_str().into());
        ui.set_axis_label(cur.axis.as_str().into());
        ui.set_accent_color(cur.color);
        ui.set_line_commands(line_path(&cur.values, 600.0, 200.0, 5.0).into());
        ui.set_area_commands(area_path(&cur.values, 600.0, 200.0, 5.0).into());

        let mut procs: Vec<_> = s.processes.iter().collect();
        procs.sort_by(|a, b| {
            b.cpu_percent
                .total_cmp(&a.cpu_percent)
                .then(b.memory_bytes.cmp(&a.memory_bytes))
        });
        let rows: Vec<ProcessRow> = procs
            .into_iter()
            .take(MAX_ROWS)
            .map(|p| ProcessRow {
                name: SharedString::from(p.name.as_str()),
                pid: p.pid.to_string().into(),
                cpu: format!("{:.1}%", p.cpu_percent).into(),
                memory: fmt_bytes(p.memory_bytes).into(),
            })
            .collect();
        self.rows.set_vec(rows);
    }
}

const ACCENTS: [(u8, u8, u8); 4] = [
    (0x7A, 0xA7, 0xFF),
    (0xF2, 0xB8, 0x4B),
    (0x5D, 0xDC, 0xB0),
    (0xC3, 0xA2, 0xFF),
];

/// Show demo data on the (still spike-shaped) Performance window. Replaced in M1-T3.
fn apply_demo(ui: &MainWindow, data: &perf::PerfData, selected: usize) {
    let color = |i: usize| Color::from_rgb_u8(ACCENTS[i].0, ACCENTS[i].1, ACCENTS[i].2);
    let cards: Vec<MetricCard> = data
        .metrics
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let tail: Vec<f64> = m.vals[30..].iter().map(|v| v / 100.0).collect();
            let mut spark = String::new();
            for (n, v) in tail.iter().enumerate() {
                let x = n as f64 * 120.0 / (tail.len() - 1) as f64;
                let y = 30.0 - v.clamp(0.0, 1.0) * 28.0;
                spark.push_str(if n == 0 { "M" } else { " L" });
                spark.push_str(&format!("{x:.1} {y:.1}"));
            }
            MetricCard {
                label: m.label.as_str().into(),
                value: m.value.as_str().into(),
                unit: m.unit.as_str().into(),
                mood: m.mood.as_str().into(),
                color: color(i),
                spark: spark.into(),
            }
        })
        .collect();
    ui.set_cards(ModelRc::new(VecModel::from(cards)));
    let cur = &data.metrics[selected];
    let vals: Vec<f64> = cur.vals.iter().map(|v| v / 100.0).collect();
    ui.set_selected(selected as i32);
    ui.set_headline(cur.headline.as_str().into());
    ui.set_big_value(cur.value.as_str().into());
    ui.set_big_unit(cur.unit.as_str().into());
    ui.set_axis_label(cur.axis.as_str().into());
    ui.set_accent_color(color(selected));
    ui.set_line_commands(line_path(&vals, 600.0, 200.0, 5.0).into());
    ui.set_area_commands(area_path(&vals, 600.0, 200.0, 5.0).into());
}

fn run_screenshot(shot: &cli::Screenshot) -> Result<(), Box<dyn std::error::Error>> {
    if shot.screen != "performance" {
        return Err(format!("unknown screen '{}'", shot.screen).into());
    }
    let view = perf::View::from_state(shot.state.as_deref())?;
    let window = screenshot::install()?;
    let ui = MainWindow::new()?;
    apply_demo(&ui, &demo::performance(), view.metric);
    ui.show()?;
    screenshot::save(&window, std::path::Path::new(&shot.out))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = cli::parse(std::env::args().skip(1))?;
    if let Some(shot) = &args.screenshot {
        return run_screenshot(shot);
    }
    let ui = MainWindow::new()?;
    let rows = Rc::new(VecModel::default());
    ui.set_processes(ModelRc::from(rows.clone()));

    if args.demo {
        apply_demo(&ui, &demo::performance(), 0);
        ui.on_pick({
            let weak = ui.as_weak();
            move |i| {
                if let Some(ui) = weak.upgrade() {
                    apply_demo(&ui, &demo::performance(), i.clamp(0, 3) as usize);
                }
            }
        });
        return Ok(ui.run()?);
    }

    let app = Rc::new(RefCell::new(App {
        stats: LiveStats::new(),
        history: History::default(),
        total_memory: 0.0,
        rows,
        snapshot: None,
    }));
    app.borrow_mut().sample();
    app.borrow().render(&ui, 0);

    ui.on_pick({
        let app = app.clone();
        let weak = ui.as_weak();
        move |i| {
            if let Some(ui) = weak.upgrade() {
                app.borrow().render(&ui, i.max(0) as usize);
            }
        }
    });

    // One sample per second; skip the work while the window is hidden.
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), {
        let weak = ui.as_weak();
        move || {
            let Some(ui) = weak.upgrade() else { return };
            if !ui.window().is_visible() {
                return;
            }
            let mut app = app.borrow_mut();
            app.sample();
            app.render(&ui, ui.get_selected().max(0) as usize);
        }
    });

    Ok(ui.run()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_history_gives_empty_paths() {
        assert_eq!(line_path(&[], 600.0, 200.0, 5.0), "");
        assert_eq!(area_path(&[], 600.0, 200.0, 5.0), "");
    }

    #[test]
    fn full_history_spans_the_width() {
        let values = vec![0.0; HISTORY];
        let p = line_path(&values, 590.0, 200.0, 5.0);
        assert!(p.starts_with("M0.0 195.0"));
        assert!(p.ends_with("L590.0 195.0"));
    }

    #[test]
    fn short_history_is_right_aligned() {
        let p = line_path(&[1.0, 1.0], 590.0, 200.0, 5.0);
        assert_eq!(p, "M580.0 5.0 L590.0 5.0");
    }

    #[test]
    fn values_are_clamped_to_the_chart() {
        let p = line_path(&[9.0, -3.0], 590.0, 200.0, 5.0);
        assert!(p.contains(" 5.0") && p.contains(" 195.0"));
    }

    #[test]
    fn rate_scale_never_drops_below_the_floor() {
        let q: VecDeque<f64> = [10.0, 20.0].into();
        assert_eq!(rate_scale(&q, MB), MB);
        let q: VecDeque<f64> = [5.0 * MB].into();
        assert_eq!(rate_scale(&q, MB), 5.0 * MB);
    }
}
