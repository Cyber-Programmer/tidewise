//! Read-only probe, run by hand: does a top-level block of the home folder equal the total shown
//! when that folder is opened?  `cargo test -p tidewise-core --test probe_home -- --ignored --nocapture`
//! It only reads (metadata), never writes or deletes.

use std::sync::atomic::AtomicBool;
use tidewise_core::scan::{scan, Progress, ScanOptions};

#[test]
#[ignore]
fn home_blocks_against_opened_totals() {
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
    let opts = ScanOptions::default();
    let root = scan(&home, &opts, &Progress::default(), &AtomicBool::new(false));
    println!(
        "home total {}  (sum of children {})",
        root.total_bytes,
        root.children.iter().map(|c| c.1).sum::<u64>()
    );
    for (path, bytes) in root.children.iter().take(8) {
        if !path.is_dir() {
            continue;
        }
        let opened = scan(path, &opts, &Progress::default(), &AtomicBool::new(false));
        println!(
            "{:<28} block {:>14}  opened {:>14}  diff {:>+12}",
            path.file_name().unwrap().to_string_lossy(),
            bytes,
            opened.total_bytes,
            opened.total_bytes as i64 - *bytes as i64
        );
    }
}
