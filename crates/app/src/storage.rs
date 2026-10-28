//! Data behind the Storage screen. Plain Rust, no Slint types.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tidewise_core::drive::Drive;

use tidewise_core::scan::ScanResult;

const GB: f64 = 1_000_000_000.0;
const MB: f64 = 1_000_000.0;

/// A rectangle as fractions (0.0–1.0) of the map it sits in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A size as the design writes it: `381 GB`, `12.6 GB`, `340 MB`, `2 TB`. Whole numbers lose the
/// `.0`; from 100 GB up there are no decimals.
pub fn fmt_size(bytes: u64) -> String {
    let gb = bytes as f64 / GB;
    if gb >= 1000.0 {
        return trim(gb / 1000.0, "TB");
    }
    if gb >= 100.0 {
        return format!("{gb:.0} GB");
    }
    if gb >= 1.0 {
        return trim(gb, "GB");
    }
    format!("{:.0} MB", bytes as f64 / MB)
}

/// One decimal, without a trailing `.0`.
fn trim(value: f64, unit: &str) -> String {
    let one = (value * 10.0).round() / 10.0;
    if one.fract() == 0.0 {
        format!("{one:.0} {unit}")
    } else {
        format!("{one:.1} {unit}")
    }
}

/// Always one decimal in GB (`6.1 GB`, `22.0 GB`): the file rows and the Trash button.
pub fn fmt_gb1(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / GB)
}

/// How long ago, as a plain amount: `3 weeks`, `8 months`, `1 year`. None for under two days.
pub fn age_amount(days: u64) -> Option<String> {
    let plural = |n: u64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    match days {
        0..=1 => None,
        2..=13 => Some(plural(days, "day")),
        14..=59 => Some(plural(days / 7, "week")),
        60..=359 => Some(plural(days / 30, "month")),
        _ => Some(plural((days / 365).max(1), "year")),
    }
}

/// `yesterday`, `3 weeks ago`, `8 months ago`.
pub fn age_ago(days: u64) -> String {
    match age_amount(days) {
        Some(amount) => format!("{amount} ago"),
        None if days == 0 => "today".into(),
        None => "yesterday".into(),
    }
}

/// Squarified treemap: `sizes` (biggest first) laid out in a box `aspect` times as wide as tall.
/// Returns one rectangle per size, in order, covering the box with no overlap.
pub fn treemap(sizes: &[u64], aspect: f64) -> Vec<Rect> {
    let total: f64 = sizes.iter().map(|&s| s as f64).sum();
    if sizes.is_empty() {
        return Vec::new();
    }
    let mut out = vec![
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        };
        sizes.len()
    ];
    // Work in a box `aspect` wide and 1 tall, with each item's area as its share of it.
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    let area: Vec<f64> = sizes
        .iter()
        .map(|&s| if total > 0.0 { s as f64 / total } else { 1.0 / sizes.len() as f64 } * aspect)
        .collect();
    let (mut x, mut y, mut w, mut h) = (0.0, 0.0, aspect, 1.0);
    let mut i = 0;
    while i < area.len() {
        let side = w.min(h);
        // Grow the row while the worst block gets no thinner.
        let worst = |row: &[f64]| {
            let sum: f64 = row.iter().sum();
            row.iter()
                .map(|a| (side * side * a / (sum * sum)).max(sum * sum / (side * side * a)))
                .fold(0.0, f64::max)
        };
        let mut end = i + 1;
        while end < area.len() && worst(&area[i..=end]) <= worst(&area[i..end]) {
            end += 1;
        }
        let sum: f64 = area[i..end].iter().sum();
        let thick = if side > 0.0 { sum / side } else { 0.0 };
        let mut offset = 0.0;
        for k in i..end {
            let len = if sum > 0.0 { side * area[k] / sum } else { 0.0 };
            let (bx, by, bw, bh) = if w >= h {
                (x, y + offset, thick, len)
            } else {
                (x + offset, y, len, thick)
            };
            out[k] = Rect {
                x: bx / aspect,
                y: by,
                w: bw / aspect,
                h: bh,
            };
            offset += len;
        }
        if w >= h {
            x += thick;
            w -= thick;
        } else {
            y += thick;
            h -= thick;
        }
        i = end;
    }
    out
}

/// One block of the folder map.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub path: Option<PathBuf>,
    pub bytes: u64,
}

/// The folder map's blocks from a scan: the biggest `max - 1` folders plus "Other" for the rest.
pub fn blocks_from(scan: &ScanResult, max: usize) -> Vec<Block> {
    let named = |path: &PathBuf, bytes: u64| Block {
        name: plain_name(path),
        path: Some(path.clone()),
        bytes,
    };
    let kept: Vec<_> = scan.children.iter().filter(|c| c.1 > 0).collect();
    if kept.len() <= max.max(1) {
        return kept.iter().map(|(p, b)| named(p, *b)).collect();
    }
    let keep = max.max(2) - 1;
    let mut blocks: Vec<_> = kept[..keep].iter().map(|(p, b)| named(p, *b)).collect();
    blocks.push(Block {
        name: "Other".into(),
        path: None,
        bytes: kept[keep..].iter().map(|c| c.1).sum(),
    });
    blocks
}

/// The folder's name; a few well-known hidden folders get a plain-language name.
fn plain_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match name.as_str() {
        ".cache" => "Caches",
        ".local" => "App data",
        ".config" => "App settings",
        ".steam" => "Steam",
        ".var" => "Flatpak app data",
        ".mozilla" => "Firefox",
        other => return other.to_string(),
    }
    .to_string()
}

/// "Downloads · opened 8 months ago": the first folder under home, then how long ago.
pub fn file_meta(home: &Path, path: &Path, days_since_use: u64) -> String {
    let place = match path.strip_prefix(home) {
        Ok(rel) if rel.components().count() <= 1 => "Home folder".to_string(),
        Ok(rel) => first_name(rel),
        Err(_) => first_name(path),
    };
    format!("{place} · opened {}", age_ago(days_since_use))
}

fn first_name(path: &Path) -> String {
    path.components()
        .find_map(|c| match c {
            std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .unwrap_or_default()
}

/// What the Move button says: `Move 6 to Trash · 22.0 GB`; names nothing when nothing is ticked.
pub fn trash_label(count: usize, bytes: u64) -> String {
    if count == 0 {
        return "Nothing ticked".into();
    }
    format!("Move {count} to Trash · {}", fmt_gb1(bytes))
}

/// Not opened for this many days counts as "not opened in 6 months".
pub const OLD_DAYS: u64 = 183;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    Scanning,
    Error,
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub name: String,
    pub size: String,
    pub note: String,
    pub hue: i32,
    pub rect: Rect,
    pub tier: i32,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FolderRow {
    pub name: String,
    pub meta: String,
    pub size: String,
    pub old: bool,
    pub checked: bool,
    pub bytes: u64,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BigFile {
    pub name: String,
    pub meta: String,
    pub size: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub parent: String,
    pub title: String,
    pub help: String,
    pub rows: Vec<FolderRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StorageData {
    pub drives: Vec<(String, bool)>,
    pub drive_name: String,
    pub used: String,
    pub free: String,
    pub bar_label: String,
    /// (share of the drive, drive-bar colour number)
    pub segs: Vec<(f64, i32)>,
    pub legend: Vec<(i32, String, String)>,
    pub state: State,
    pub message: String,
    pub map_title: String,
    pub tiles: Vec<Tile>,
    pub hint: Option<(String, String)>,
    pub folder: Option<Folder>,
    pub big: Vec<BigFile>,
    pub big_note: String,
}

/// (share of the drive, drive-bar colour number)
pub type Seg = (f64, i32);
/// (colour number, label, value)
pub type LegendRow = (i32, String, String);

/// Type size and padding step of a block, from its share of the map.
pub fn tile_tier(share: f64) -> i32 {
    match share {
        s if s >= 0.33 => 0,
        s if s >= 0.2 => 1,
        s if s >= 0.12 => 2,
        _ => 3,
    }
}

/// Colour number of each block: Downloads is amber and "Other" grey, the rest cycle.
pub fn assign_hues(names: &[&str]) -> Vec<i32> {
    let mut cycle = [0, 2, 3, 5].into_iter().cycle();
    names
        .iter()
        .map(|n| match *n {
            "Downloads" => 1,
            "Other" => 4,
            _ => cycle.next().unwrap_or(0),
        })
        .collect()
}

/// What kind of thing this is, in plain words.
pub fn kind_of(name: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "Folder";
    }
    let lower = name.to_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "iso" | "img" | "deb" | "rpm" | "appimage" => "Installer",
        "qcow2" | "vdi" | "vmdk" | "vhd" => "Virtual machine disk",
        "zip" | "gz" | "tgz" | "xz" | "bz2" | "7z" | "rar" | "tar" | "zst" => "Archive",
        "mkv" | "mp4" | "mov" | "avi" | "webm" => "Video",
        "jpg" | "jpeg" | "png" | "raw" | "heic" => "Picture",
        _ => "File",
    }
}

/// One line of the drill-in list. Things not opened in six months start ticked.
pub fn folder_row(
    name: &str,
    path: &Path,
    bytes: u64,
    is_dir: bool,
    days_unused: u64,
) -> FolderRow {
    let old = days_unused >= OLD_DAYS;
    let kind = kind_of(name, is_dir);
    let meta = match (old, age_amount(days_unused)) {
        (true, Some(amount)) => format!("{kind} · not opened in {amount}"),
        _ => format!("{kind} · opened {}", age_ago(days_unused)),
    };
    FolderRow {
        name: name.to_string(),
        meta,
        size: fmt_gb1(bytes),
        old,
        checked: old,
        bytes,
        path: Some(path.to_path_buf()),
    }
}

/// How many rows are ticked and how many bytes they hold.
pub fn selection(rows: &[FolderRow]) -> (usize, u64) {
    rows.iter()
        .filter(|r| r.checked)
        .fold((0, 0), |(n, b), r| (n + 1, b + r.bytes))
}

/// The drive switcher's label: `Main drive · 476 GB`, `Data · 2 TB`.
pub fn drive_label(mount_point: &str, name: &str, total: u64) -> String {
    let who = if mount_point == "/" {
        "Main drive".to_string()
    } else {
        Path::new(mount_point)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| name.to_string())
    };
    format!("{who} · {}", fmt_size(total))
}

/// Drive bar from what we can measure: the part of the drive holding the scanned folder, the rest
/// of what is used, and (implicitly) the free space. `scanned` is None while measuring.
pub fn drive_bar(
    total: u64,
    used: u64,
    scanned: Option<(&str, u64)>,
) -> (Vec<Seg>, Vec<LegendRow>) {
    if total == 0 {
        return (Vec::new(), Vec::new());
    }
    let share = |b: u64| (b as f64 / total as f64).clamp(0.0, 1.0);
    let Some((label, files)) = scanned else {
        return (vec![(share(used), 5)], Vec::new());
    };
    let files = files.min(used);
    let rest = used - files;
    let mut segs = vec![(share(files), 0)];
    let mut legend = vec![(0, label.to_string(), fmt_size(files))];
    if rest > 0 {
        segs.push((share(rest), 1));
        legend.push((1, "Apps & system".into(), fmt_size(rest)));
    }
    (segs, legend)
}

/// Spoken version of the bar, for screen readers.
pub fn bar_label(legend: &[(i32, String, String)], free: &str) -> String {
    let parts: Vec<_> = legend
        .iter()
        .map(|(_, l, v)| format!("{} {v}", l.to_lowercase()))
        .collect();
    format!(
        "Drive usage: {}{}free {free}",
        parts.join(", "),
        if parts.is_empty() { "" } else { ", " }
    )
}

/// How far the scan of the picked drive has come.
pub enum Phase<'a> {
    Scanning { bytes: u64 },
    Done(&'a ScanResult),
}

/// One entry of the drill-in folder, as read from disk.
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub is_dir: bool,
    pub days_unused: u64,
}

/// Whole days between two moments; 0 if `then` is in the future.
pub fn days_since(now: SystemTime, then: SystemTime) -> u64 {
    now.duration_since(then).map_or(0, |d| d.as_secs() / 86_400)
}

/// Bytes of not-opened-in-6-months files found below `dir`.
pub fn stale_bytes_under(scan: &ScanResult, dir: &Path) -> u64 {
    scan.stale_files
        .iter()
        .filter(|f| f.path.starts_with(dir))
        .map(|f| f.bytes)
        .sum()
}

/// Everything the Storage screen shows for real data.
pub fn live_data(
    drives: &[Drive],
    picked: usize,
    phase: Phase,
    folder: Option<(&str, &str, Vec<Entry>)>,
    home: &Path,
    now: SystemTime,
) -> Option<StorageData> {
    let drive = drives.get(picked)?;
    let tabs = drives
        .iter()
        .enumerate()
        .map(|(i, d)| (drive_label(&d.mount_point, &d.name, d.total), i == picked))
        .collect();
    let free = format!("{} free", fmt_size(drive.free));
    let mut data = StorageData {
        drives: tabs,
        drive_name: if drive.mount_point == "/" {
            "Main drive".into()
        } else {
            drive_label(&drive.mount_point, &drive.name, 0)
                .split(" \u{b7} ")
                .next()
                .unwrap_or("Drive")
                .to_string()
        },
        used: fmt_size(drive.used),
        free: free.clone(),
        bar_label: String::new(),
        segs: Vec::new(),
        legend: Vec::new(),
        state: State::Ready,
        message: String::new(),
        map_title: String::new(),
        tiles: Vec::new(),
        hint: None,
        folder: None,
        big: Vec::new(),
        big_note: "Your own files always go to Trash first, never deleted straight away.".into(),
    };
    let scan = match phase {
        Phase::Scanning { bytes } => {
            (data.segs, data.legend) = drive_bar(drive.total, drive.used, None);
            data.bar_label = bar_label(&data.legend, &free);
            data.state = State::Scanning;
            data.message = format!("{} looked at so far", fmt_size(bytes));
            return Some(data);
        }
        Phase::Done(scan) => scan,
    };
    let label = if scan.root == home {
        "Your files"
    } else {
        "Files"
    };
    (data.segs, data.legend) = drive_bar(drive.total, drive.used, Some((label, scan.total_bytes)));
    data.bar_label = bar_label(&data.legend, &free);
    if scan.total_bytes == 0 {
        data.state = if scan.skipped > 0 {
            State::Error
        } else {
            State::Empty
        };
        data.message = if scan.skipped > 0 {
            "Tidewise isn't allowed to read this folder. Nothing was changed.".into()
        } else {
            "This folder is empty.".into()
        };
        return Some(data);
    }
    data.map_title = format!("{label} \u{b7} {}", fmt_size(scan.total_bytes));
    let blocks = blocks_from(scan, 6);
    let sizes: Vec<u64> = blocks.iter().map(|b| b.bytes).collect();
    let names: Vec<&str> = blocks.iter().map(|b| b.name.as_str()).collect();
    let hues = assign_hues(&names);
    for ((block, rect), hue) in blocks.iter().zip(treemap(&sizes, 2.0)).zip(hues) {
        let stale = block
            .path
            .as_deref()
            .map_or(0, |p| stale_bytes_under(scan, p));
        let note = if stale >= 100_000_000 {
            format!("{} not opened in 6 months", fmt_size(stale))
        } else {
            String::new()
        };
        if block.name == "Downloads" && stale >= 100_000_000 {
            data.hint = Some((
                format!(
                    "Downloads has {} of files you haven't opened in 6 months.",
                    fmt_size(stale)
                ),
                "Review them".into(),
            ));
        }
        data.tiles.push(Tile {
            name: block.name.clone(),
            size: fmt_size(block.bytes),
            note,
            hue,
            rect,
            tier: tile_tier(block.bytes as f64 / scan.total_bytes as f64),
            path: block.path.clone(),
        });
    }
    data.big = scan
        .biggest_files
        .iter()
        .take(5)
        .map(|f| BigFile {
            name: f
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            meta: file_meta(home, &f.path, days_since(now, f.last_used)),
            size: fmt_gb1(f.bytes),
            path: Some(f.path.clone()),
        })
        .collect();
    if scan.used_modified_time {
        data.big_note.push_str(" Dates are when files last changed, because this drive doesn't record when they are opened.");
    }
    if let Some((parent, title, entries)) = folder {
        let rows = entries
            .iter()
            .map(|e| folder_row(&e.name, &e.path, e.bytes, e.is_dir, e.days_unused))
            .collect();
        data.folder = Some(Folder {
            parent: parent.into(),
            title: title.into(),
            help: "Biggest first. Items you haven't opened in 6 months are ticked for you; change anything you like.".into(),
            rows,
        });
    }
    Some(data)
}

/// The toast after a Trash action: what moved, or what could not and that nothing was deleted.
pub fn toast_text(moved: usize, failed: &[(String, String)]) -> String {
    let files = |n: usize| format!("{n} file{}", if n == 1 { "" } else { "s" });
    match (moved, failed.first()) {
        (0, None) => "Nothing was ticked.".into(),
        (n, None) => format!("{} moved to Trash", files(n)),
        (0, Some((name, why))) => {
            format!("Couldn't move {name} to Trash: {why} Nothing was deleted.")
        }
        (n, Some(_)) => format!(
            "{} moved to Trash; {} couldn't be moved and are still where they were.",
            files(n),
            files(failed.len())
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 1;

    #[test]
    fn sizes_are_written_like_the_design() {
        assert_eq!(fmt_size(381_000_000_000), "381 GB");
        assert_eq!(fmt_size(12_600_000_000), "12.6 GB");
        assert_eq!(fmt_size(52_000_000_000), "52 GB");
        assert_eq!(fmt_size(142_300_000_000), "142 GB");
        assert_eq!(fmt_size(340_000_000), "340 MB");
        assert_eq!(fmt_size(2_000_000_000_000), "2 TB");
        assert_eq!(fmt_size(0), "0 MB");
    }

    #[test]
    fn file_sizes_always_have_one_decimal() {
        assert_eq!(fmt_gb1(6_100_000_000), "6.1 GB");
        assert_eq!(fmt_gb1(22_000_000_000), "22.0 GB");
        assert_eq!(fmt_gb1(100_000_000), "0.1 GB");
        assert_eq!(fmt_gb1(12_449_000_000), "12.4 GB");
    }

    #[test]
    fn ages_read_like_the_design() {
        assert_eq!(age_amount(1), None);
        assert_eq!(age_ago(0), "today");
        assert_eq!(age_ago(DAY), "yesterday");
        assert_eq!(age_ago(5), "5 days ago");
        assert_eq!(age_ago(21), "3 weeks ago");
        assert_eq!(age_ago(8 * 30 + 5), "8 months ago");
        assert_eq!(age_ago(365), "1 year ago");
        assert_eq!(age_ago(800), "2 years ago");
        assert_eq!(age_amount(21).as_deref(), Some("3 weeks"));
        assert_eq!(age_amount(300).as_deref(), Some("10 months"));
        assert_eq!(age_amount(400).as_deref(), Some("1 year"));
    }

    #[test]
    fn age_boundaries_do_not_skip_a_unit() {
        assert_eq!(age_ago(13), "13 days ago");
        assert_eq!(age_ago(14), "2 weeks ago");
        assert_eq!(age_ago(59), "8 weeks ago");
        assert_eq!(age_ago(60), "2 months ago");
        assert_eq!(age_ago(359), "11 months ago");
        assert_eq!(age_ago(360), "1 year ago");
    }

    fn area(r: &Rect) -> f64 {
        r.w * r.h
    }

    #[test]
    fn treemap_areas_are_proportional_and_fill_the_box() {
        let sizes = [52, 38, 21, 12, 10, 9];
        let rects = treemap(&sizes, 2.0);
        assert_eq!(rects.len(), 6);
        let total: f64 = rects.iter().map(area).sum();
        assert!((total - 1.0).abs() < 1e-9, "fills the box: {total}");
        for (r, s) in rects.iter().zip(sizes) {
            assert!((area(r) - s as f64 / 142.0).abs() < 1e-9, "{r:?} for {s}");
            assert!(
                r.x >= -1e-9 && r.y >= -1e-9 && r.x + r.w <= 1.0 + 1e-9 && r.y + r.h <= 1.0 + 1e-9
            );
        }
    }

    #[test]
    fn treemap_blocks_do_not_overlap() {
        let rects = treemap(&[50, 30, 30, 20, 10, 5, 5, 1], 1.5);
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let ox = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let oy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(ox <= 1e-9 || oy <= 1e-9, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn treemap_handles_nothing_zero_and_one() {
        assert!(treemap(&[], 2.0).is_empty());
        assert_eq!(
            treemap(&[5], 2.0),
            vec![Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0
            }]
        );
        let r = treemap(&[10, 0, 0], 2.0);
        assert_eq!(r.len(), 3);
        assert!(r.iter().all(|r| r.w.is_finite() && r.h.is_finite()));
    }

    fn scan_with(children: &[(&str, u64)]) -> ScanResult {
        ScanResult {
            root: "/home/ali".into(),
            total_bytes: children.iter().map(|c| c.1).sum(),
            children: children
                .iter()
                .map(|(n, b)| (PathBuf::from("/home/ali").join(n), *b))
                .collect(),
            ..ScanResult::default()
        }
    }

    #[test]
    fn the_map_keeps_the_biggest_folders_and_groups_the_rest_as_other() {
        let scan = scan_with(&[("a", 50), ("b", 40), ("c", 30), ("d", 20), ("e", 10)]);
        let blocks = blocks_from(&scan, 4);
        let names: Vec<_> = blocks.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c", "Other"]);
        assert_eq!(blocks[3].bytes, 30);
        assert!(blocks[3].path.is_none());
        assert_eq!(blocks.iter().map(|b| b.bytes).sum::<u64>(), 150);
    }

    #[test]
    fn a_short_list_has_no_other_and_empty_folders_are_dropped() {
        let scan = scan_with(&[("a", 50), ("b", 0)]);
        let blocks = blocks_from(&scan, 6);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "a");
        assert!(blocks_from(&ScanResult::default(), 6).is_empty());
    }

    #[test]
    fn hidden_folders_get_plain_names() {
        let scan = scan_with(&[(".cache", 50), (".local", 40), (".weird-tool", 30)]);
        let names: Vec<_> = blocks_from(&scan, 6).into_iter().map(|b| b.name).collect();
        assert_eq!(names, ["Caches", "App data", ".weird-tool"]);
    }

    #[test]
    fn file_meta_names_the_folder_under_home() {
        let home = Path::new("/home/ali");
        assert_eq!(
            file_meta(home, Path::new("/home/ali/Downloads/x.iso"), 245),
            "Downloads · opened 8 months ago"
        );
        assert_eq!(
            file_meta(home, Path::new("/home/ali/Projects/a/b/node_modules/x"), 21),
            "Projects · opened 3 weeks ago"
        );
        assert_eq!(
            file_meta(home, Path::new("/home/ali/notes.txt"), 0),
            "Home folder · opened today"
        );
        assert_eq!(
            file_meta(home, Path::new("/elsewhere/x"), 1),
            "elsewhere · opened yesterday"
        );
    }

    #[test]
    fn the_trash_button_names_what_it_does() {
        assert_eq!(trash_label(6, 22_000_000_000), "Move 6 to Trash · 22.0 GB");
        assert_eq!(trash_label(1, 6_100_000_000), "Move 1 to Trash · 6.1 GB");
        assert_eq!(trash_label(0, 0), "Nothing ticked");
    }

    #[test]
    fn block_size_steps_follow_the_share_of_the_map() {
        assert_eq!(tile_tier(0.366), 0);
        assert_eq!(tile_tier(0.268), 1);
        assert_eq!(tile_tier(0.148), 2);
        assert_eq!(tile_tier(0.085), 3);
        assert_eq!(tile_tier(0.33), 0);
        assert_eq!(tile_tier(0.2), 1);
        assert_eq!(tile_tier(0.12), 2);
    }

    #[test]
    fn downloads_is_amber_other_is_grey_the_rest_cycle() {
        assert_eq!(
            assign_hues(&[
                "Steam",
                "Downloads",
                "Videos",
                "Pictures",
                "Other",
                "Documents"
            ]),
            [0, 1, 2, 3, 4, 5]
        );
        assert_eq!(assign_hues(&["a", "b", "c", "d", "e"]), [0, 2, 3, 5, 0]);
    }

    #[test]
    fn rows_not_opened_for_six_months_start_ticked() {
        let p = Path::new("/home/ali/Downloads/ubuntu.iso");
        let old = folder_row("ubuntu.iso", p, 6_100_000_000, false, 245);
        assert!(old.old && old.checked);
        assert_eq!(old.meta, "Installer · not opened in 8 months");
        assert_eq!(old.size, "6.1 GB");
        let young = folder_row("datasets", p, 12_400_000_000, true, 1);
        assert!(!young.old && !young.checked);
        assert_eq!(young.meta, "Folder · opened yesterday");
        // The boundary: 182 days is still recent, 183 is old.
        assert!(!folder_row("x", p, 1, false, 182).old);
        assert!(folder_row("x", p, 1, false, 183).old);
    }

    #[test]
    fn the_selection_counts_only_ticked_rows() {
        let p = Path::new("/x");
        let mut rows = vec![
            folder_row("a", p, 5, false, 400),
            folder_row("b", p, 7, false, 1),
            folder_row("c", p, 11, false, 400),
        ];
        assert_eq!(selection(&rows), (2, 16));
        rows[1].checked = true;
        rows[0].checked = false;
        assert_eq!(selection(&rows), (2, 18));
        assert_eq!(selection(&[]), (0, 0));
    }

    #[test]
    fn drive_labels_name_the_main_drive_and_others_by_folder() {
        assert_eq!(
            drive_label("/", "/dev/nvme0n1p2", 476_000_000_000),
            "Main drive · 476 GB"
        );
        assert_eq!(
            drive_label("/run/media/ali/Data", "/dev/sdb1", 2_000_000_000_000),
            "Data · 2 TB"
        );
        assert_eq!(drive_label("/", "x", 0), "Main drive · 0 MB");
    }

    #[test]
    fn the_drive_bar_splits_used_space_and_never_exceeds_the_drive() {
        let (segs, legend) = drive_bar(1000, 800, Some(("Your files", 300)));
        assert_eq!(segs, [(0.3, 0), (0.5, 1)]);
        assert_eq!(legend.len(), 2);
        // A scan can see more than the drive reports (hard links, snapshots): clamp, never overflow.
        let (segs, _) = drive_bar(1000, 800, Some(("Your files", 5000)));
        assert!(segs.iter().map(|s| s.0).sum::<f64>() <= 0.8 + 1e-9);
        assert_eq!(drive_bar(1000, 800, None).0, [(0.8, 5)]);
        assert!(drive_bar(0, 0, None).0.is_empty());
    }

    #[test]
    fn the_spoken_bar_ends_with_the_free_space() {
        let (_, legend) = drive_bar(
            1_000_000_000_000,
            800_000_000_000,
            Some(("Your files", 300_000_000_000)),
        );
        let text = bar_label(&legend, "95 GB");
        assert!(text.starts_with("Drive usage: your files 300 GB"), "{text}");
        assert!(text.ends_with("free 95 GB"), "{text}");
    }

    #[test]
    fn kinds_are_plain_words() {
        assert_eq!(kind_of("a.ISO", false), "Installer");
        assert_eq!(kind_of("disk.qcow2", false), "Virtual machine disk");
        assert_eq!(kind_of("x.tar.gz", false), "Archive");
        assert_eq!(kind_of("x.iso", true), "Folder");
        assert_eq!(kind_of("noext", false), "File");
    }

    fn drive(mount: &str, total: u64, used: u64) -> Drive {
        Drive {
            name: "/dev/x".into(),
            mount_point: mount.into(),
            fs_type: "ext4".into(),
            total,
            used,
            free: total - used,
        }
    }

    fn live(scan: &ScanResult) -> StorageData {
        let drives = [drive("/", 500_000_000_000, 400_000_000_000)];
        live_data(
            &drives,
            0,
            Phase::Done(scan),
            None,
            Path::new("/home/ali"),
            SystemTime::now(),
        )
        .unwrap()
    }

    #[test]
    fn a_scan_becomes_tiles_a_bar_and_a_downloads_hint() {
        let mut scan = scan_with(&[("Downloads", 38_000_000_000), ("Videos", 21_000_000_000)]);
        scan.stale_files = vec![tidewise_core::scan::FileEntry {
            path: "/home/ali/Downloads/a.iso".into(),
            bytes: 22_000_000_000,
            last_used: SystemTime::UNIX_EPOCH,
        }];
        let d = live(&scan);
        assert_eq!(d.state, State::Ready);
        assert_eq!(d.map_title, "Your files \u{b7} 59 GB");
        assert_eq!(d.tiles.len(), 2);
        assert_eq!(d.tiles[0].note, "22 GB not opened in 6 months");
        assert!(d.hint.unwrap().0.starts_with("Downloads has 22 GB"));
        assert_eq!(d.used, "400 GB");
        assert_eq!(d.free, "100 GB free");
    }

    #[test]
    fn scanning_shows_progress_and_no_tiles() {
        let drives = [drive("/", 500_000_000_000, 400_000_000_000)];
        let d = live_data(
            &drives,
            0,
            Phase::Scanning {
                bytes: 3_000_000_000,
            },
            None,
            Path::new("/h"),
            SystemTime::now(),
        )
        .unwrap();
        assert_eq!(d.state, State::Scanning);
        assert_eq!(d.message, "3 GB looked at so far");
        assert!(d.tiles.is_empty());
    }

    #[test]
    fn an_unreadable_or_empty_folder_says_which() {
        let mut scan = ScanResult {
            root: "/home/ali".into(),
            skipped: 1,
            ..ScanResult::default()
        };
        assert_eq!(live(&scan).state, State::Error);
        scan.skipped = 0;
        assert_eq!(live(&scan).state, State::Empty);
        assert!(live_data(
            &[],
            0,
            Phase::Scanning { bytes: 0 },
            None,
            Path::new("/"),
            SystemTime::now()
        )
        .is_none());
    }

    #[test]
    fn stale_bytes_only_count_files_inside_the_folder() {
        let mut scan = scan_with(&[]);
        let f = |p: &str, b| tidewise_core::scan::FileEntry {
            path: p.into(),
            bytes: b,
            last_used: SystemTime::UNIX_EPOCH,
        };
        scan.stale_files = vec![
            f("/home/ali/Downloads/a", 5),
            f("/home/ali/Downloads2/b", 7),
        ];
        assert_eq!(
            stale_bytes_under(&scan, Path::new("/home/ali/Downloads")),
            5
        );
    }

    #[test]
    fn days_since_never_goes_negative() {
        let now = SystemTime::now();
        assert_eq!(
            days_since(now, now + std::time::Duration::from_secs(9999)),
            0
        );
        assert_eq!(
            days_since(now, now - std::time::Duration::from_secs(3 * 86_400 + 5)),
            3
        );
    }

    #[test]
    fn the_toast_says_what_happened_in_plain_words() {
        assert_eq!(toast_text(3, &[]), "3 files moved to Trash");
        assert_eq!(toast_text(1, &[]), "1 file moved to Trash");
        assert_eq!(toast_text(0, &[]), "Nothing was ticked.");
        let why = ("a.iso".to_string(), "It is on another drive.".to_string());
        assert_eq!(
            toast_text(0, std::slice::from_ref(&why)),
            "Couldn't move a.iso to Trash: It is on another drive. Nothing was deleted."
        );
        assert!(toast_text(2, &[why]).contains("1 file couldn't be moved"));
    }
}
