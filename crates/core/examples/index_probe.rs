//! R8-Q08 measurement, not shipped: how much memory and time does a filename index of 200 000
//! files need? Builds a synthetic tree in a temp folder (removed when the program ends), then
//! indexes it in one of two layouts and prints the numbers.
//!
//!   cargo run --release -p tidewise-core --example index_probe -- naive|compact [files]
//!
//! The tree is created under `$PROBE_DIR` (default `target/tmp`); nothing outside it is read.

use std::error::Error;
use std::fs::{self, File};
use std::path::Path;
use std::time::Instant;

/// Proportional set size of this process in KiB, from `/proc/self/smaps_rollup`.
fn pss_kb() -> u64 {
    fs::read_to_string("/proc/self/smaps_rollup")
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("Pss:"))
                .and_then(|v| v.split_whitespace().next()?.parse().ok())
        })
        .unwrap_or(0)
}

/// Highest resident size so far in KiB, from `/proc/self/status`.
fn hwm_kb() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("VmHWM:"))
                .and_then(|v| v.split_whitespace().next()?.parse().ok())
        })
        .unwrap_or(0)
}

const STEMS: [&str; 6] = [
    "IMG",
    "Screenshot from 2026-11-0",
    "report-final",
    "notes",
    "DSC",
    "document",
];
const EXTS: [&str; 5] = ["jpg", "png", "txt", "pdf", "odt"];

/// `files` empty files in 3 levels of folders, 100 per leaf folder.
fn make_tree(root: &Path, files: usize) -> Result<usize, Box<dyn Error>> {
    let mut made = 0;
    'outer: for a in 0..20 {
        for b in 0..10 {
            for c in 0..100 {
                let dir = root.join(format!("Documents-{a}/project-{b}/folder-{c}"));
                fs::create_dir_all(&dir)?;
                for i in 0..100 {
                    if made == files {
                        break 'outer;
                    }
                    let name = format!("{}-{:05}.{}", STEMS[i % 6], made, EXTS[i % 5]);
                    File::create(dir.join(name))?;
                    made += 1;
                }
            }
        }
    }
    Ok(made)
}

/// Layout 1: one `String` with the whole relative path per file.
#[derive(Default)]
struct Naive {
    paths: Vec<String>,
}

/// Layout 2: every folder once, every file as (name in one big buffer, folder number).
#[derive(Default)]
struct Compact {
    names: String,
    files: Vec<(u32, u32, u32)>,   // name start, name length, folder
    folders: Vec<(u32, u32, u32)>, // name start, name length, parent folder (u32::MAX = root)
}

fn walk_naive(dir: &Path, rel: &str, out: &mut Naive) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(kind) = e.file_type() else {
            continue;
        };
        let here = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if kind.is_dir() {
            if !name.starts_with('.') {
                walk_naive(&e.path(), &here, out);
            }
        } else if kind.is_file() {
            out.paths.push(here);
        }
    }
}

fn walk_compact(dir: &Path, parent: u32, out: &mut Compact) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(kind) = e.file_type() else {
            continue;
        };
        let start = out.names.len() as u32;
        if kind.is_dir() {
            if name.starts_with('.') {
                continue;
            }
            out.names.push_str(&name);
            let id = out.folders.len() as u32;
            out.folders.push((start, name.len() as u32, parent));
            walk_compact(&e.path(), id, out);
        } else if kind.is_file() {
            out.names.push_str(&name);
            out.files.push((start, name.len() as u32, parent));
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "naive".into());
    let files: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(200_000);
    let base: std::path::PathBuf =
        std::env::var_os("PROBE_DIR").map_or_else(|| "target/tmp".into(), Into::into);
    fs::create_dir_all(&base)?;
    let tmp = tempfile::Builder::new()
        .prefix("index-probe-")
        .tempdir_in(&base)?;

    let t = Instant::now();
    let made = make_tree(tmp.path(), files)?;
    println!("tree: {made} files made in {} ms", t.elapsed().as_millis());

    let before = pss_kb();
    let t = Instant::now();
    let (count, matches, search_us, bytes, after) = match mode.as_str() {
        "naive" => {
            let mut idx = Naive::default();
            walk_naive(tmp.path(), "", &mut idx);
            let built = t.elapsed();
            let live = pss_kb();
            let q = Instant::now();
            let m = idx
                .paths
                .iter()
                .filter(|p| p.to_lowercase().contains("report"))
                .count();
            println!("build: {} ms", built.as_millis());
            let bytes: usize = idx.paths.iter().map(|p| p.len()).sum();
            (idx.paths.len(), m, q.elapsed().as_micros(), bytes, live)
        }
        "compact" => {
            let mut idx = Compact::default();
            walk_compact(tmp.path(), u32::MAX, &mut idx);
            let built = t.elapsed();
            let live = pss_kb();
            let q = Instant::now();
            let m = idx
                .files
                .iter()
                .filter(|(s, l, _)| {
                    idx.names[*s as usize..(*s + *l) as usize]
                        .to_lowercase()
                        .contains("report")
                })
                .count();
            println!("build: {} ms", built.as_millis());
            let bytes = idx.names.len() + (idx.files.len() + idx.folders.len()) * 12;
            (idx.files.len(), m, q.elapsed().as_micros(), bytes, live)
        }
        other => return Err(format!("unknown mode {other}").into()),
    };
    println!(
        "{mode}: {count} files indexed, {matches} match 'report' in {search_us} us, payload {:.1} MB, PSS {:.1} MB -> {:.1} MB (+{:.1} MB)",
        bytes as f64 / 1e6,
        before as f64 / 1024.0,
        after as f64 / 1024.0,
        (after as f64 - before as f64) / 1024.0
    );
    println!("peak resident (VmHWM): {:.1} MB", hwm_kb() as f64 / 1024.0);
    let t = Instant::now();
    drop(tmp);
    println!("tree removed in {} ms", t.elapsed().as_millis());
    Ok(())
}
