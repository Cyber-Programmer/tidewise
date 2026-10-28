//! `tidewise --screenshot` renders headless at exactly 1360×960 and is deterministic.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn run(out: &PathBuf, extra: &[&str]) {
    run_screen("performance", out, extra);
}

fn run_screen(screen: &str, out: &PathBuf, extra: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_tidewise"))
        .args(["--screenshot", screen])
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

#[test]
fn every_performance_variant_renders() {
    let dir = tempfile::tempdir().unwrap();
    let mut seen = Vec::new();
    for state in ["details", "memory", "memory-details", "network", "disk"] {
        let out = dir.path().join(format!("{state}.png"));
        run(&out, &["--state", state]);
        seen.push(fs::read(&out).unwrap());
    }
    for (i, a) in seen.iter().enumerate() {
        for b in &seen[i + 1..] {
            assert_ne!(a, b, "two variants look identical");
        }
    }
}

#[test]
fn height_can_be_raised_for_tall_artboards() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tall.png");
    run(&out, &["--state", "details", "--height", "1005"]);
    assert_eq!(png_size(&fs::read(&out).unwrap()), (1360, 1005));
}

#[test]
fn every_processes_variant_renders_and_differs() {
    let dir = tempfile::tempdir().unwrap();
    let mut seen = Vec::new();
    for state in [
        None,
        Some("details"),
        Some("apps"),
        Some("protected-selected"),
    ] {
        let out = dir
            .path()
            .join(format!("{}.png", state.unwrap_or("default")));
        let extra: Vec<&str> = state.iter().flat_map(|s| ["--state", *s]).collect();
        run_screen("processes", &out, &extra);
        let bytes = fs::read(&out).unwrap();
        assert_eq!(png_size(&bytes), (1360, 960));
        seen.push(bytes);
    }
    for (i, a) in seen.iter().enumerate() {
        for b in &seen[i + 1..] {
            assert_ne!(a, b, "two variants look identical");
        }
    }
}

#[test]
fn an_unknown_screen_or_state_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("x.png");
    for args in [["--screenshot", "nope"], ["--screenshot", "processes"]] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tidewise"));
        cmd.args(args).arg("--out").arg(&out);
        if args[1] == "processes" {
            cmd.args(["--state", "bogus"]);
        }
        assert!(!cmd.status().unwrap().success());
        assert!(!out.exists());
    }
}

#[test]
fn home_renders_and_unfinished_pages_say_so() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home.png");
    run_screen("home", &home, &[]);
    assert_eq!(png_size(&fs::read(&home).unwrap()), (1360, 960));
    let mut pages = vec![fs::read(&home).unwrap()];
    for screen in [
        "clean-up", "apps", "storage", "startup", "services", "history", "settings",
    ] {
        let out = dir.path().join(format!("{screen}.png"));
        run_screen(screen, &out, &[]);
        pages.push(fs::read(&out).unwrap());
    }
    for (i, a) in pages.iter().enumerate() {
        for b in &pages[i + 1..] {
            assert_ne!(a, b, "two pages look identical");
        }
    }
}

/// Rows (top to bottom) of the sidebar's right-hand strip that have the active-item colour.
fn active_nav_rows(png: &std::path::Path) -> Vec<u32> {
    const NAV_ACTIVE_BG: [u8; 3] = [0x1E, 0x23, 0x2C];
    let img = image::open(png).unwrap().to_rgb8();
    (0..img.height())
        .filter(|&y| img.get_pixel(196, y).0 == NAV_ACTIVE_BG)
        .collect()
}

#[test]
fn settings_is_inside_the_window_at_600_px_tall() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("settings.png");
    run_screen("settings", &out, &["--height", "600"]);
    let rows = active_nav_rows(&out);
    let (first, last) = (rows.first().copied(), rows.last().copied());
    assert!(
        rows.len() >= 36,
        "the active Settings item is not (fully) visible: {} rows, {first:?}..{last:?}",
        rows.len()
    );
    assert_eq!(
        rows.len() as u32,
        last.unwrap() - first.unwrap() + 1,
        "one block"
    );
    assert!(last.unwrap() < 600 - 8, "Settings touches the window edge");
}

#[test]
fn settings_item_is_the_same_at_960_px() {
    // Guards the check above: at the design height the same item is found.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("settings.png");
    run_screen("settings", &out, &[]);
    assert!(active_nav_rows(&out).len() >= 36);
}

#[test]
fn storage_renders_both_states_and_rejects_unknown_ones() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
    run_screen("storage", &a, &[]);
    run_screen("storage", &b, &["--state", "downloads"]);
    assert_eq!(png_size(&fs::read(&a).unwrap()), (1360, 960));
    assert_ne!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    let status = Command::new(env!("CARGO_BIN_EXE_tidewise"))
        .args(["--screenshot", "storage", "--state", "nope", "--out"])
        .arg(dir.path().join("c.png"))
        .env("TIDEWISE_RENDERER", "software")
        .status()
        .unwrap();
    assert!(!status.success());
}
