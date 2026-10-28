//! Sample data for `--demo` and screenshots, copied from the artboards' script blocks in
//! `docs/design/screens/*.dc.html`.

use crate::home::{Apps, HomeData, Hue, Icon, Scan, Suggestion, Tile};
use crate::perf::{Fact, Metric, PerfData, TopRow};
use crate::procs::{Kind, ProcData, Row};
use crate::storage::StorageData;

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

/// The Home artboard: score 82, three suggestions, four tiles, 143 apps.
pub fn home() -> HomeData {
    let tile = |label: &str, mood: &str, mood_hue, value: &str, unit: &str, hue, spark, bar| Tile {
        label: label.into(),
        mood: mood.into(),
        mood_hue,
        value: value.into(),
        unit: unit.into(),
        hue,
        spark,
        bar,
    };
    let suggestion = |icon, title: &str, body: &str, action: &str, page| Suggestion {
        icon,
        title: title.into(),
        body: body.into(),
        action: action.into(),
        page,
    };
    HomeData {
        last_checked: "Last checked 2 minutes ago".into(),
        scan: Some(Scan {
            score: 82,
            tag: "Good shape".into(),
            headline: "Your PC is running well. 3 things could free up space or speed it up.".into(),
            sub: "Nothing changes until you review it. Every fix shows exactly what it will touch first.".into(),
            suggestions: vec![
                suggestion(Icon::Clean, "12.6 GB of junk files can be removed", "Old installer downloads, app caches and logs. Apps rebuild what they need.", "Clean up", 1),
                suggestion(Icon::Warning, "Your main drive is 80% full", "Downloads is the biggest folder you control (38 GB). 22 GB of it is older than 6 months.", "See what's using space", 3),
                suggestion(Icon::Chip, "Firefox is using 2.1 GB of memory", "Normal with many tabs open. Closing a few will make other apps feel quicker.", "View", 5),
            ],
        }),
        tiles: vec![
            tile("Processor", "Idle", Hue::Cpu, "14%", "", Hue::Cpu, Some(vec![(0.0, 28.0), (14.0, 26.0), (28.0, 29.0), (42.0, 22.0), (56.0, 25.0), (70.0, 18.0), (84.0, 24.0), (98.0, 27.0), (112.0, 20.0), (126.0, 23.0), (140.0, 12.0), (154.0, 21.0), (168.0, 25.0), (182.0, 22.0), (200.0, 24.0)]), None),
            tile("Memory", "Busy", Hue::Memory, "9.8", "of 16 GB", Hue::Memory, None, Some(0.61)),
            tile("Main drive", "80% full", Hue::Memory, "95 GB", "free", Hue::Disk, None, Some(0.80)),
            tile("Network", "Wi-Fi", Hue::Muted, "2.4", "MB/s down", Hue::Network, Some(vec![(0.0, 30.0), (14.0, 30.0), (28.0, 26.0), (42.0, 14.0), (56.0, 9.0), (70.0, 12.0), (84.0, 20.0), (98.0, 28.0), (112.0, 29.0), (126.0, 24.0), (140.0, 16.0), (154.0, 10.0), (168.0, 13.0), (182.0, 22.0), (200.0, 26.0)]), None),
        ],
        apps: Some(Apps {
            title: "143 apps, one list".into(),
            sub: "Tidewise finds apps from every place Linux installs them, so you can remove them the same way.".into(),
            chips: vec![("System".into(), 112), ("Flatpak".into(), 24), ("AppImage".into(), 5), ("Snap".into(), 2)],
        }),
    }
}

/// The Storage artboard's sample data. `downloads` shows the drill-in (Storage--downloads.png).
pub fn storage(downloads: bool) -> StorageData {
    use crate::storage::{BigFile, Folder, FolderRow, Rect, State, Tile};
    let r = |x, y, w, h| Rect { x, y, w, h };
    let tile = |name: &str, size: &str, note: &str, hue, rect, tier| Tile {
        name: name.into(),
        size: size.into(),
        note: note.into(),
        hue,
        rect,
        tier,
        path: None,
    };
    let legend = |hue, label: &str, value: &str| (hue, label.to_string(), value.to_string());
    let row = |name: &str, meta: &str, gb: f64, old: bool, checked: bool| FolderRow {
        name: name.into(),
        meta: meta.into(),
        size: format!("{gb:.1} GB"),
        old,
        checked,
        bytes: (gb * 1e9) as u64,
        path: None,
        kind: crate::storage::RowKind::File,
        label: name.into(),
        focused: false,
    };
    let big = |name: &str, meta: &str, size: &str| BigFile {
        name: name.into(),
        meta: meta.into(),
        size: size.into(),
        path: None,
    };
    let rows = vec![
        row(
            "datasets",
            "Folder \u{b7} opened yesterday",
            12.4,
            false,
            false,
        ),
        row(
            "old-vm-disk.qcow2",
            "Virtual machine disk \u{b7} not opened in 1 year",
            6.3,
            true,
            true,
        ),
        row(
            "ubuntu-24.04-desktop-amd64.iso",
            "Installer \u{b7} not opened in 8 months",
            6.1,
            true,
            true,
        ),
        row(
            "vacation-photos.zip",
            "Archive \u{b7} not opened in 9 months",
            4.2,
            true,
            true,
        ),
        row(
            "Setup files (.deb, .rpm) \u{b7} 23 files",
            "Installers already used \u{b7} 6+ months",
            3.9,
            true,
            true,
        ),
        row(
            "lecture-recordings",
            "Folder \u{b7} opened 3 weeks ago",
            3.6,
            false,
            false,
        ),
        row(
            "archlinux-x86_64.iso",
            "Installer \u{b7} not opened in 10 months",
            1.4,
            true,
            true,
        ),
        row(
            "Obsidian-1.8.AppImage",
            "Older copy of an app you have \u{b7} 7 months",
            0.1,
            true,
            true,
        ),
    ];
    StorageData {
        drives: vec![("Main drive \u{b7} 476 GB".into(), true), ("Data \u{b7} 2 TB".into(), false)],
        drive_name: "Main drive".into(),
        used: "381 GB".into(),
        free: "95 GB free".into(),
        bar_label: "Drive usage: your files 142 GB, apps and system 98 GB, Docker images 54 GB, Flatpak apps 41 GB, junk 12.6 GB, other 33 GB, free 95 GB".into(),
        segs: vec![(0.298, 0), (0.206, 1), (0.113, 2), (0.086, 3), (0.026, 4), (0.07, 5)],
        legend: vec![
            legend(0, "Your files", "142 GB"),
            legend(1, "Apps & system", "98 GB"),
            legend(2, "Docker images", "54 GB"),
            legend(3, "Flatpak apps", "41 GB"),
            legend(4, "Junk you can clean", "12.6 GB"),
            legend(5, "Other", "33 GB"),
        ],
        state: State::Ready,
        message: String::new(),
        map_title: "Your files \u{b7} 142 GB".into(),
        tiles: vec![
            tile("Steam games", "52 GB", "9 games installed", 0, r(0.0, 0.0, 0.366, 1.0), 0),
            tile("Downloads", "38 GB", "22 GB not opened in 6 months", 1, r(0.366, 0.0, 0.408, 0.66), 1),
            tile("Videos", "21 GB", "", 2, r(0.774, 0.0, 0.226, 0.66), 2),
            tile("Pictures", "12 GB", "", 3, r(0.366, 0.66, 0.2454, 0.34), 3),
            tile("Other", "10 GB", "", 4, r(0.6114, 0.66, 0.2045, 0.34), 3),
            tile("Documents", "9 GB", "", 5, r(0.8159, 0.66, 0.1841, 0.34), 3),
        ],
        hint: Some((
            "Downloads has 22 GB of files you haven't opened in 6 months. Old installers (.iso, .deb, .AppImage) make up most of it.".into(),
            "Review them".into(),
        )),
        folder: downloads.then(|| Folder {
            crumbs: vec![
                crate::nav::Crumb { label: "Your files".into(), target: Some(0) },
                crate::nav::Crumb { label: "Downloads \u{b7} 38 GB".into(), target: None },
            ],
            help: "Biggest first. Items you haven't opened in 6 months are ticked for you; change anything you like.".into(),
            rows,
        }),
        big: vec![
            big("ubuntu-24.04-desktop-amd64.iso", "Downloads \u{b7} opened 8 months ago", "6.1 GB"),
            big("screen-recording-march.mkv", "Videos \u{b7} opened 7 months ago", "3.8 GB"),
            big("backup-2025.tar.gz", "Documents \u{b7} opened 1 year ago", "2.9 GB"),
            big("node_modules \u{b7} 14 folders", "Projects \u{b7} rebuilt with npm install", "1.9 GB"),
            big("archlinux-x86_64.iso", "Downloads \u{b7} opened 10 months ago", "1.4 GB"),
        ],
        big_note: "Your own files always go to Trash first, never deleted straight away.".into(),
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

    #[test]
    fn home_demo_matches_the_artboard() {
        let h = home();
        let scan = h.scan.unwrap();
        assert_eq!((scan.score, scan.suggestions.len()), (82, 3));
        assert_eq!(h.tiles.len(), 4);
        assert_eq!(h.tiles[0].spark.as_ref().unwrap().len(), 15);
        assert_eq!(h.apps.unwrap().chips.iter().map(|c| c.1).sum::<u32>(), 143);
    }
}
