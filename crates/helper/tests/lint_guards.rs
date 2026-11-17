//! Golden rules 3 and 7 are enforced by clippy's `disallowed-methods` (`clippy.toml`). This file guards the guard
//! (review 5.8 R-14, threat T-20): the ban list can only change on purpose, and no source file switches the lint off.
//!
//! Both are text checks, a prompt for review and not a proof of absence: a lint can also be switched off in a
//! `Cargo.toml` `[lints]` table or on the command line. Today the only such place is `tests/container/Cargo.toml`
//! (the container crate runs programs and removes files inside a throwaway container, by design), and CI runs
//! `cargo clippy -- -D warnings` without extra flags.

use std::fs;
use std::path::{Path, PathBuf};

/// The calls `clippy.toml` bans, sorted. Adding, removing or renaming one is a change to golden rules 3 and 7, so it
/// is made here, in the same commit, where a reviewer sees it. A rustix path that stops resolving after a rustix
/// update would otherwise stop banning without any warning.
const BANNED: [&str; 11] = [
    "rustix::fs::rename",
    "rustix::fs::renameat",
    "rustix::fs::renameat_with",
    "rustix::fs::rmdir",
    "rustix::fs::unlink",
    "rustix::fs::unlinkat",
    "std::fs::remove_dir",
    "std::fs::remove_dir_all",
    "std::fs::remove_file",
    "std::fs::rename",
    "std::process::Command::new",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `path = "..."` values of the non-comment lines of a `clippy.toml`, sorted.
fn banned_paths(clippy_toml: &str) -> Vec<String> {
    let mut paths: Vec<String> = clippy_toml
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let after = line.split("path = \"").nth(1)?;
            Some(after.split('"').next()?.to_string())
        })
        .collect();
    paths.sort();
    paths
}

#[test]
fn clippy_toml_still_bans_every_listed_call() {
    let text = fs::read_to_string(workspace_root().join("clippy.toml")).unwrap();
    assert_eq!(banned_paths(&text), BANNED);
}

#[test]
fn the_ban_list_reader_sees_a_removed_or_commented_out_entry() {
    let two = "disallowed-methods = [\n    { path = \"std::fs::remove_file\", reason = \"x\" },\n    { path = \"std::fs::rename\", reason = \"y\" },\n]\n";
    assert_eq!(
        banned_paths(two),
        ["std::fs::remove_file", "std::fs::rename"]
    );
    let commented = two.replace(
        "    { path = \"std::fs::rename\"",
        "    # { path = \"std::fs::rename\"",
    );
    assert_eq!(banned_paths(&commented), ["std::fs::remove_file"]);
}

/// The spellings that switch the ban (or every lint) off. Built from pieces so this file does not contain them.
fn switches() -> Vec<String> {
    ["allow", "warn"]
        .iter()
        .flat_map(|level| {
            ["clippy::disallowed_methods", "clippy::all", "warnings"]
                .map(|lint| format!("{level}({lint}"))
        })
        .collect()
}

/// The lines of `source` that are not comments and switch off the ban or all lints, trimmed. `expect(` is the allowed
/// way: it fails clippy when the exception no longer matches anything.
fn lines_that_switch_the_lint_off(source: &str) -> Vec<String> {
    let switches = switches();
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//"))
        .filter(|line| switches.iter().any(|switch| line.contains(switch.as_str())))
        .map(str::to_string)
        .collect()
}

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if !matches!(name.as_str(), "target" | "corpus" | "artifacts" | ".git") {
                rust_files(&path, found);
            }
        } else if name.ends_with(".rs") {
            found.push(path);
        }
    }
}

#[test]
fn the_scanner_finds_each_way_of_switching_the_ban_off_and_leaves_expect_alone() {
    for bad in [
        "#[allow(clippy::disallowed_methods)]",
        "#![allow(clippy::disallowed_methods, reason = \"x\")]",
        "    #![allow(clippy::all)]",
        "#![allow(warnings)]",
        "#[warn(clippy::disallowed_methods)]",
    ] {
        assert_eq!(lines_that_switch_the_lint_off(bad).len(), 1, "{bad}");
    }
    for fine in [
        "#[expect(clippy::disallowed_methods, reason = \"the one runner\")]",
        "// #[allow(clippy::disallowed_methods)] is what not to write",
        "#![allow(dead_code)]",
        "#[deny(clippy::disallowed_methods)]",
    ] {
        assert!(lines_that_switch_the_lint_off(fine).is_empty(), "{fine}");
    }
}

#[test]
fn no_source_file_switches_the_disallowed_methods_lint_off() {
    let root = workspace_root();
    let mut files = Vec::new();
    for folder in ["crates", "tests", "fuzz"] {
        rust_files(&root.join(folder), &mut files);
    }
    assert!(files.len() > 50, "found only {} files", files.len());
    let mut offenders = Vec::new();
    for file in files {
        // This file holds the spellings as test data.
        if file.ends_with("crates/helper/tests/lint_guards.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        for line in lines_that_switch_the_lint_off(&text) {
            offenders.push(format!("{}: {line}", file.display()));
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

/// Review 5.8 B-07. The root helper links these four crates, so none of them may hold `unsafe` code. The helper's own
/// binary (`main.rs`) is a separate crate root and needs its own line.
#[test]
fn every_crate_the_root_helper_links_forbids_unsafe_code() {
    let root = workspace_root();
    for file in [
        "crates/helper/src/lib.rs",
        "crates/helper/src/main.rs",
        "crates/helper-protocol/src/lib.rs",
        "crates/names/src/lib.rs",
        "crates/pacman/src/lib.rs",
    ] {
        let text = fs::read_to_string(root.join(file)).unwrap();
        assert!(
            text.lines()
                .any(|line| line.trim() == "#![forbid(unsafe_code)]"),
            "{file} does not forbid unsafe code"
        );
    }
}
