//! The AppImage build: the desktop file, the AppDir the script assembles, and (when
//! `appimagetool` is installed) the real AppImage starting headless.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn script(args: &[&str]) -> std::process::Output {
    Command::new(root().join("packaging/appimage/build.sh"))
        .args(args)
        .output()
        .expect("run build.sh")
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    (
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    )
}

#[test]
fn the_desktop_file_has_what_launchers_need() {
    let text =
        fs::read_to_string(root().join("packaging/appimage/io.github.tidewise.desktop")).unwrap();
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{key}=")))
            .unwrap_or_else(|| panic!("missing {key}"))
            .to_string()
    };
    assert!(text.starts_with("[Desktop Entry]\n"));
    assert_eq!(get("Type"), "Application");
    assert_eq!(get("Name"), "Tidewise");
    assert_eq!(get("Exec"), "tidewise");
    assert_eq!(get("Icon"), "io.github.tidewise");
    assert_eq!(get("Terminal"), "false");
    assert!(get("Categories").ends_with(';'), "list values end with ;");
}

#[test]
fn the_script_assembles_an_appdir_that_starts_and_draws_home() {
    let dir = tempfile::tempdir().unwrap();
    let out = script(&[
        "--appdir-only",
        "--binary",
        env!("CARGO_BIN_EXE_tidewise"),
        "--out",
        dir.path().to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let appdir = dir.path().join("AppDir");
    for file in [
        "AppRun",
        "usr/bin/tidewise",
        "io.github.tidewise.desktop",
        "io.github.tidewise.svg",
        "usr/share/applications/io.github.tidewise.desktop",
        "usr/share/icons/hicolor/scalable/apps/io.github.tidewise.svg",
    ] {
        assert!(appdir.join(file).exists(), "missing {file}");
    }
    let png = dir.path().join("home.png");
    let run = Command::new(appdir.join("AppRun"))
        .args(["--screenshot", "home", "--out"])
        .arg(&png)
        .env("TIDEWISE_RENDERER", "software")
        .status()
        .unwrap();
    assert!(run.success());
    assert_eq!(png_size(&fs::read(&png).unwrap()), (1360, 960));
}

#[test]
fn the_script_refuses_bad_input_clearly() {
    assert!(!script(&["--nonsense"]).status.success());
    let out = script(&["--appdir-only", "--binary", "/definitely/not/here"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no runnable binary"));
}

/// The real thing. Needs `appimagetool` on PATH (not installed on every machine); skipped otherwise.
#[test]
fn the_appimage_starts_headless_and_produces_a_1360_by_960_png() {
    let have_tool = Command::new("sh")
        .args(["-c", "command -v appimagetool"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !have_tool {
        eprintln!("skipped: appimagetool is not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let out = script(&[
        "--binary",
        env!("CARGO_BIN_EXE_tidewise"),
        "--out",
        dir.path().to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let image = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .last()
        .unwrap()
        .to_string();
    let png = dir.path().join("home.png");
    let run = Command::new(&image)
        .args([
            "--appimage-extract-and-run",
            "--screenshot",
            "home",
            "--out",
        ])
        .arg(&png)
        .env("TIDEWISE_RENDERER", "software")
        .status()
        .unwrap();
    assert!(run.success());
    assert_eq!(png_size(&fs::read(&png).unwrap()), (1360, 960));
}
