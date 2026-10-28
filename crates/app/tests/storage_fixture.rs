//! The fixture tree that later Storage tests and screenshots are built on.
#![allow(dead_code)]

#[path = "support/fixture.rs"]
mod fixture;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

fn walk(
    dir: &Path,
    depth: usize,
    deepest: &mut usize,
    out: &mut Vec<(std::path::PathBuf, fs::Metadata)>,
) {
    *deepest = (*deepest).max(depth);
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        let m = fs::symlink_metadata(&p).unwrap();
        if m.is_dir() {
            walk(&p, depth + 1, deepest, out);
        }
        out.push((p, m));
    }
}

fn all(fx: &fixture::Fixture) -> (usize, Vec<(std::path::PathBuf, fs::Metadata)>) {
    let (mut deepest, mut v) = (0, Vec::new());
    walk(&fx.root, 0, &mut deepest, &mut v);
    (deepest, v)
}

#[test]
fn the_tree_is_at_least_four_folders_deep() {
    let fx = fixture::build();
    assert!(all(&fx).0 >= 4);
}

#[test]
fn one_folder_has_forty_or_more_items() {
    let fx = fixture::build();
    let most = fs::read_dir(fx.root.join("Documents/Photos"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert!(most >= 40, "{most} items");
}

#[test]
fn du_reports_a_file_over_1_gb_and_files_under_1_mb() {
    let fx = fixture::build();
    let big = fx.root.join("Documents/Archive/disk.img");
    let out = Command::new("/usr/bin/du")
        .args(["-sB1"])
        .arg(&big)
        .output()
        .expect("du");
    let bytes: u64 = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    assert!(bytes > 1_000_000_000, "du says {bytes}");
    let small = all(&fx)
        .1
        .iter()
        .filter(|(_, m)| m.is_file() && m.blocks() * 512 < 1_000_000 && m.blocks() > 0)
        .count();
    assert!(small >= 10, "{small} small files");
}

#[test]
fn it_has_a_symlink_and_a_hard_link_across_folders() {
    let fx = fixture::build();
    let (_, v) = all(&fx);
    assert!(v.iter().any(|(_, m)| m.file_type().is_symlink()));
    assert!(v.iter().any(|(_, m)| m.is_file() && m.nlink() > 1));
}

/// `cargo test -p tidewise-app --test storage_fixture keep -- --ignored --nocapture` leaves a fixture
/// behind and prints its path, for taking screenshots by hand. Remove it with `rm -rf` afterwards.
#[test]
#[ignore]
fn keep() {
    let fx = fixture::build();
    let root = fx.root.clone();
    let _ = fx.dir.keep();
    println!("FIXTURE {}", root.display());
}
