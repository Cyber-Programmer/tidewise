//! `tidewise --screenshot` renders headless at exactly 1360×960 and is deterministic.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn run(out: &PathBuf, extra: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_tidewise"))
        .args(["--screenshot", "performance"])
        .args(extra)
        .arg("--out")
        .arg(out)
        .env("TIDEWISE_RENDERER", "software")
        .status()
        .unwrap();
    assert!(status.success(), "tidewise exited with {status}");
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (w, h)
}

#[test]
fn screenshot_is_1360_by_960() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("a.png");
    run(&out, &[]);
    assert_eq!(png_size(&fs::read(&out).unwrap()), (1360, 960));
}

#[test]
fn two_runs_give_identical_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
    run(&a, &[]);
    run(&b, &[]);
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
}

#[test]
fn a_state_variant_changes_the_picture() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
    run(&a, &[]);
    run(&b, &["--state", "memory"]);
    assert_ne!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
}
