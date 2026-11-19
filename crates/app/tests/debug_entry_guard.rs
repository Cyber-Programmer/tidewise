//! The debug-only way into the group install (WBS 6.2; the real way in is WBS 6.12) must not exist in a release
//! build. Two source checks, like the lint guards:
//!
//! 1. the variable name appears in exactly one file, `src/debug_entry.rs`;
//! 2. every line of `src/main.rs` that mentions `debug_entry` sits directly under `#[cfg(debug_assertions)]`.
//!
//! Together: the only code that reads the variable is compiled only when `debug_assertions` is on, which a
//! release build turns off. (The release binary is also checked by hand with `strings`.)

use std::fs;
use std::path::{Path, PathBuf};

const VARIABLE: &str = "TIDEWISE_DEBUG_INSTALL_GROUP";

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every file under `dir` (recursively), except the build output.
fn files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

#[test]
fn only_the_debug_entry_file_reads_the_variable() {
    let mut files = Vec::new();
    for folder in ["src", "ui"] {
        files_under(&app_dir().join(folder), &mut files);
    }
    files.push(app_dir().join("Cargo.toml"));
    files.push(app_dir().join("build.rs"));
    let mut mentioning: Vec<String> = files
        .iter()
        .filter(|path| {
            fs::read(path)
                .map(|bytes| String::from_utf8_lossy(&bytes).contains(VARIABLE))
                .unwrap_or(false)
        })
        .map(|path| path.strip_prefix(app_dir()).unwrap().display().to_string())
        .collect();
    mentioning.sort();
    assert_eq!(mentioning, ["src/debug_entry.rs"]);
}

#[test]
fn main_compiles_the_debug_entry_only_with_debug_assertions() {
    let main = fs::read_to_string(app_dir().join("src/main.rs")).unwrap();
    let lines: Vec<&str> = main.lines().collect();
    let mut mentions = 0;
    for (index, line) in lines.iter().enumerate() {
        if !line.contains("debug_entry") {
            continue;
        }
        mentions += 1;
        let before = lines[..index]
            .iter()
            .rev()
            .map(|l| l.trim())
            .find(|l| !l.is_empty());
        assert_eq!(
            before,
            Some("#[cfg(debug_assertions)]"),
            "line {} of main.rs mentions debug_entry without #[cfg(debug_assertions)] right above it",
            index + 1
        );
    }
    assert!(mentions > 0, "main.rs does not use the debug entry at all");
}
