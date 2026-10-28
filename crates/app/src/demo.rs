//! Sample data for `--demo` and screenshots, copied from the artboards' script blocks in
//! `docs/design/screens/*.dc.html`.

use crate::perf::{Fact, Metric, PerfData, TopRow};
use crate::procs::{Kind, ProcData, Row};

/// The artboard's `series(seed, base, amp, spikeAt, spikeH)`: 60 samples on a 0–100 scale.
pub fn series(seed: f64, base: f64, amp: f64, spike_at: &[usize], spike_h: f64) -> Vec<f64> {
    (0..60)
        .map(|i| {
            let t = i as f64;
            let mut v =
                base + (t * 0.37 + seed).sin() * amp + (t * 1.13 + seed * 2.0).sin() * amp * 0.6;
            for &at in spike_at {
                let dist = (i as f64 - at as f64).abs();
                if dist < 3.0 {
                    v += spike_h * (1.0 - dist / 3.0);
                }
            }
            v.clamp(1.0, 98.0)
        })
        .collect()
}

fn fact(k: &str, v: &str) -> Fact {
    Fact {
        k: k.into(),
        v: v.into(),
    }
}

fn top(rows: &[(&str, &str, f64)]) -> Vec<TopRow> {
    rows.iter()
        .map(|&(name, value, width)| TopRow {
            name: name.into(),
            value: value.into(),
            width,
        })
        .collect()
}

/// The four metrics of the Performance artboard, with its exact copy and numbers.
pub fn performance() -> PerfData {
    let metrics = vec![
        Metric {
            label: "Processor".into(),
            value: "14".into(),
            unit: "%".into(),
            mood: "Idle".into(),
            axis: "100%".into(),
            headline: "Mostly idle".into(),
            explain: "Your processor has plenty of room. The short spikes are apps opening or pages loading, which is normal. If it sits near the top for minutes, check Processes to see which app is busy.".into(),
            vals: series(1.0, 14.0, 5.0, &[18, 44], 30.0),
            facts: vec![
                fact("Model", "AMD Ryzen 5 5600H"),
                fact("Cores · threads", "6 · 12"),
                fact("Speed now", "3.3 GHz"),
                fact("Temperature", "52 °C · normal"),
            ],
            top: top(&[("Firefox", "6.2%", 0.62), ("Plasma desktop", "2.1%", 0.21), ("Visual Studio Code", "1.8%", 0.18)]),
        },
        Metric {
            label: "Memory".into(),
            value: "9.8".into(),
            unit: "of 16 GB".into(),
            mood: "Busy".into(),
            axis: "16 GB".into(),
            headline: "Busy, but fine".into(),
            explain: "Apps are using 9.8 GB. Linux also borrows spare memory to make things faster and hands it back the moment an app needs it, so a high number alone is not a problem.".into(),
            vals: series(2.0, 60.0, 2.0, &[], 0.0),
            facts: vec![
                fact("Used by apps", "9.8 GB"),
                fact("Borrowed as cache", "4.1 GB (given back on demand)"),
                fact("Completely free", "2.1 GB"),
                fact("Swap", "0.4 of 8 GB (zram)"),
            ],
            top: top(&[("Firefox", "2.1 GB", 0.84), ("Visual Studio Code", "1.3 GB", 0.52), ("Steam", "0.9 GB", 0.36)]),
        },
        Metric {
            label: "Network".into(),
            value: "2.4".into(),
            unit: "MB/s down".into(),
            mood: "Downloading".into(),
            axis: "10 MB/s".into(),
            headline: "Steam is downloading".into(),
            explain: "Most of the traffic is a Steam game update. Uploads are light. If the internet feels slow in other apps, pausing the download will help.".into(),
            vals: series(3.0, 35.0, 14.0, &[30], 25.0),
            facts: vec![
                fact("Connection", "Wi-Fi · wlan0"),
                fact("Upload now", "180 KB/s"),
                fact("Today", "3.1 GB down · 240 MB up"),
                fact("Local address", "192.168.1.24"),
            ],
            top: top(&[("Steam", "1.9 MB/s", 0.80), ("Firefox", "420 KB/s", 0.18), ("Spotify", "80 KB/s", 0.04)]),
        },
        Metric {
            label: "Disk".into(),
            value: "12".into(),
            unit: "MB/s".into(),
            mood: "Quiet".into(),
            axis: "100% busy".into(),
            headline: "Barely working".into(),
            explain: "Your drive is only busy 3% of the time. If this stays near the top for a long time, apps will open slowly. Usually that means a large copy, an update or the file indexer.".into(),
            vals: series(4.0, 6.0, 3.0, &[30, 31], 18.0),
            facts: vec![
                fact("Drive", "NVMe SSD · 476 GB"),
                fact("Read · write now", "11 · 1 MB/s"),
                fact("Busy", "3%"),
                fact("Health check", "Needs admin to read"),
            ],
            top: top(&[("Steam", "9 MB/s", 0.75), ("File indexer", "2 MB/s", 0.17), ("Firefox", "0.6 MB/s", 0.05)]),
        },
    ];
    PerfData {
        subtitle: "Live · last 60 seconds. Your PC is calm; memory is the busiest part.".into(),
        metrics,
        threads: vec![
            22.0, 8.0, 31.0, 5.0, 12.0, 46.0, 9.0, 4.0, 18.0, 7.0, 26.0, 11.0,
        ],
    }
}

struct Demo {
    id: &'static str,
    name: &'static str,
    kind: &'static str,
    group: Kind,
    count: &'static str,
    cpu: f64,
    mem: &'static str,
    mem_hot: bool,
    disk: &'static str,
    net: &'static str,
    pid: &'static str,
    user: &'static str,
    av: (u8, u8, u8),
    protected: bool,
    note: &'static str,
    cmd: &'static str,
}

/// The eight rows of the Processes artboard, with its exact copy.
pub fn processes() -> ProcData {
    const A: Kind = Kind::App;
    const B: Kind = Kind::Background;
    #[rustfmt::skip]
    let all = [
        Demo { id: "firefox", name: "Firefox", kind: "App", group: A, count: "14 processes", cpu: 6.2, mem: "2.1 GB", mem_hot: true, disk: "0.6 MB/s", net: "420 KB/s", pid: "2231", user: "you", av: (0xFF, 0xB3, 0x8A), protected: false, note: "Web browser. Ending it closes all windows; Firefox can restore your tabs next time.", cmd: "/usr/lib/firefox/firefox  ·  14 child processes" },
        Demo { id: "code", name: "Visual Studio Code", kind: "App", group: A, count: "9 processes", cpu: 1.8, mem: "1.3 GB", mem_hot: true, disk: "0", net: "0", pid: "3410", user: "you", av: (0x7A, 0xA7, 0xFF), protected: false, note: "Code editor. Unsaved files may be lost if you end it.", cmd: "/opt/visual-studio-code/code  ·  9 child processes" },
        Demo { id: "steam", name: "Steam", kind: "App", group: A, count: "6 processes", cpu: 1.2, mem: "0.9 GB", mem_hot: false, disk: "9 MB/s", net: "1.9 MB/s", pid: "4102", user: "you", av: (0x9F, 0xC1, 0xFF), protected: false, note: "Downloading a game update. Ending it pauses the download.", cmd: "flatpak run com.valvesoftware.Steam" },
        Demo { id: "spotify", name: "Spotify", kind: "App", group: A, count: "5 processes", cpu: 0.6, mem: "410 MB", mem_hot: false, disk: "0", net: "80 KB/s", pid: "5120", user: "you", av: (0x8B, 0xEA, 0xCB), protected: false, note: "Music app. Safe to end; music stops.", cmd: "/snap/spotify/current/usr/share/spotify/spotify" },
        Demo { id: "docker", name: "Docker", kind: "Background service", group: B, count: "4 processes", cpu: 0.2, mem: "640 MB", mem_hot: false, disk: "0", net: "0", pid: "812", user: "root", av: (0x7F, 0xB2, 0xE5), protected: false, note: "Runs containers. Stopping it frees memory if you are not using containers. Needs your password.", cmd: "/usr/bin/dockerd -H fd://" },
        Demo { id: "baloo", name: "File indexer", kind: "Background service", group: B, count: "1 process", cpu: 0.3, mem: "120 MB", mem_hot: false, disk: "2 MB/s", net: "0", pid: "1544", user: "you", av: (0xC3, 0xA2, 0xFF), protected: false, note: "Makes file search fast. Safe to pause; it picks up later.", cmd: "/usr/bin/baloo_file" },
        Demo { id: "plasma", name: "Plasma desktop", kind: "System", group: Kind::System, count: "3 processes", cpu: 2.1, mem: "380 MB", mem_hot: false, disk: "0", net: "0", pid: "1203", user: "you", av: (0xB4, 0xBA, 0xC5), protected: true, note: "Draws your desktop, panel and windows. Ending it would close your session, so Tidewise won\u{2019}t.", cmd: "/usr/bin/plasmashell" },
        Demo { id: "pipewire", name: "Sound system", kind: "System", group: Kind::System, count: "3 processes", cpu: 0.4, mem: "48 MB", mem_hot: false, disk: "0", net: "0", pid: "988", user: "you", av: (0xF2, 0xB8, 0x4B), protected: true, note: "PipeWire plays every sound on your PC. Ending it would cut audio, so Tidewise won\u{2019}t.", cmd: "/usr/bin/pipewire  ·  pipewire-pulse  ·  wireplumber" },
    ];
    let rows = all
        .iter()
        .map(|d| Row {
            id: d.id.into(),
            name: d.name.into(),
            kind: d.group,
            sub: format!("{} · {}", d.kind, d.count),
            sub_details: format!("{} · PID {} · user {}", d.kind, d.pid, d.user),
            color: d.av,
            protected: d.protected,
            cpu: format!("{:.1}%", d.cpu),
            cpu_hot: d.cpu >= 5.0,
            mem: d.mem.into(),
            mem_hot: d.mem_hot,
            disk: d.disk.into(),
            net: d.net.into(),
            note: d.note.into(),
            cmd: d.cmd.into(),
        })
        .collect();
    ProcData {
        subtitle: "214 running, grouped by the app they belong to.".into(),
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-5
    }

    // Expected values were printed by running the artboard's own JavaScript with node.
    #[test]
    fn cpu_series_matches_the_design_script() {
        let s = series(1.0, 14.0, 5.0, &[18, 44], 30.0);
        assert_eq!(s.len(), 60);
        for (i, want) in [
            (0, 20.935247),
            (1, 18.934317),
            (18, 47.880767),
            (30, 8.829616),
            (44, 41.979746),
            (59, 8.989142),
        ] {
            assert!(near(s[i], want), "cpu[{i}] = {} want {want}", s[i]);
        }
        assert!(near(s.iter().sum::<f64>(), 1033.987781));
    }

    #[test]
    fn memory_series_matches_the_design_script() {
        let s = series(2.0, 60.0, 2.0, &[], 0.0);
        assert!(near(s[0], 60.910432) && near(s[44], 58.552072));
        assert!(near(s.iter().sum::<f64>(), 3595.818207));
    }

    #[test]
    fn network_series_matches_the_design_script() {
        let s = series(3.0, 35.0, 14.0, &[30], 25.0);
        assert!(near(s[30], 80.777308) && near(s[59], 27.451816));
        assert!(near(s.iter().sum::<f64>(), 2110.733873));
    }

    #[test]
    fn disk_series_matches_the_design_script() {
        let s = series(4.0, 6.0, 3.0, &[30, 31], 18.0);
        assert!(near(s[30], 36.143971) && near(s[18], 3.049222));
        assert!(near(s.iter().sum::<f64>(), 455.469378));
    }

    #[test]
    fn values_stay_between_1_and_98() {
        let s = series(9.0, 90.0, 30.0, &[10], 80.0);
        assert!(s.iter().all(|v| (1.0..=98.0).contains(v)));
    }

    #[test]
    fn processes_demo_has_the_artboards_eight_rows() {
        let d = processes();
        assert_eq!(d.rows.len(), 8);
        assert_eq!(d.rows[0].sub, "App · 14 processes");
        assert_eq!(d.rows[0].sub_details, "App · PID 2231 · user you");
        assert_eq!(d.rows[0].cpu, "6.2%");
        assert!(d.rows[0].cpu_hot && !d.rows[1].cpu_hot);
        assert!(d.rows[6].protected && d.rows[7].protected);
        assert!(d.rows[6].note.ends_with("so Tidewise won\u{2019}t."));
    }
}
