//! Tauri spike: the Performance screen. The frontend is plain HTML/CSS/JS in `ui/`;
//! all numbers come from `tidewise-core` through one command, `sample`.

use std::sync::Mutex;

use serde::Serialize;
use tauri::State;
use tidewise_core::stats::LiveStats;

#[derive(Serialize)]
struct Process {
    pid: u32,
    name: String,
    cpu_percent: f32,
    memory_bytes: u64,
}

#[derive(Serialize)]
struct Sample {
    cpu_percent: f32,
    memory_used: u64,
    memory_total: u64,
    disk_read_per_sec: Option<f64>,
    disk_write_per_sec: Option<f64>,
    net_down_per_sec: Option<f64>,
    net_up_per_sec: Option<f64>,
    processes: Vec<Process>,
}

const MAX_ROWS: usize = 200;

#[tauri::command]
fn sample(stats: State<'_, Mutex<LiveStats>>) -> Result<Sample, String> {
    let snapshot = stats
        .lock()
        .map_err(|_| "stats lock poisoned".to_string())?
        .sample();
    let mut processes: Vec<Process> = snapshot
        .processes
        .into_iter()
        .map(|p| Process {
            pid: p.pid,
            name: p.name,
            cpu_percent: p.cpu_percent,
            memory_bytes: p.memory_bytes,
        })
        .collect();
    processes.sort_by(|a, b| {
        b.cpu_percent
            .total_cmp(&a.cpu_percent)
            .then(b.memory_bytes.cmp(&a.memory_bytes))
    });
    processes.truncate(MAX_ROWS);
    Ok(Sample {
        cpu_percent: snapshot.cpu_percent,
        memory_used: snapshot.memory_used,
        memory_total: snapshot.memory_total,
        disk_read_per_sec: snapshot.disk_read_per_sec,
        disk_write_per_sec: snapshot.disk_write_per_sec,
        net_down_per_sec: snapshot.net_down_per_sec,
        net_up_per_sec: snapshot.net_up_per_sec,
        processes,
    })
}

fn main() {
    let result = tauri::Builder::default()
        .manage(Mutex::new(LiveStats::new()))
        .invoke_handler(tauri::generate_handler![sample])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("tauri error: {err}");
        std::process::exit(1);
    }
}
