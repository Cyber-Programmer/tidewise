//! Golden rules 7 and 9 as a test: the helper's code can only change files in the places listed here, and the lock
//! file `db.lck` is only ever looked at.
//!
//! The test reads the helper's own source (everything before `#[cfg(test)]` in each file, comment lines left out) and
//! collects every line that creates, removes, renames, copies or writes a file, or that mentions the lock. That list
//! must be exactly the one below. So a new line that touches a file, or that could remove the lock, fails this test until
//! someone reads it and adds it here on purpose: it is a prompt for review, not a proof of absence.

use std::fs;
use std::path::Path;

/// What counts as touching a file or the lock.
const WORDS: [&str; 14] = [
    "remove_file",
    "remove_dir",
    "create_dir",
    "DirBuilder",
    "OpenOptions",
    "File::create",
    "fs::write",
    "fs::rename",
    "set_permissions",
    "fs::copy",
    "symlink(",
    "db.lck",
    "DATABASE_LOCK",
    "lock_path",
];

/// Every such line of the non-test code: (file, line without its indentation).
fn found() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for entry in fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap();
        for line in code.lines() {
            let line = line.trim();
            if !line.starts_with("//") && WORDS.iter().any(|word| line.contains(word)) {
                found.push((name.clone(), line.to_string()));
            }
        }
    }
    found.sort();
    found
}

fn expected() -> Vec<(String, String)> {
    let mut list: Vec<(&str, &str)> = vec![
        // The lock: its path, and the one place that looks at it (symlink_metadata never opens, creates or follows).
        (
            "runner.rs",
            "pub const DATABASE_LOCK: &str = \"/var/lib/pacman/db.lck\";",
        ),
        ("runner.rs", "lock_path: PathBuf,"),
        ("runner.rs", "lock_path: PathBuf::from(DATABASE_LOCK),"),
        ("runner.rs", "match fs::symlink_metadata(&self.lock_path) {"),
        // Removes the two output files of the run that just ended.
        ("runner.rs", "let _ = fs::remove_file(path);"),
        // The work folder (/run/tidewise-helper): created if missing, files created with O_EXCL, own leftovers removed.
        (
            "workdir.rs",
            "use std::fs::{self, DirBuilder, File, OpenOptions};",
        ),
        (
            "workdir.rs",
            "use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};",
        ),
        (
            "workdir.rs",
            "match DirBuilder::new().mode(0o700).create(path) {",
        ),
        (
            "workdir.rs",
            "if is_regular_file && !process_is_alive(pid) && fs::remove_file(entry.path()).is_ok() {",
        ),
        ("workdir.rs", "let _ = fs::remove_file(&out_path);"),
        ("workdir.rs", "OpenOptions::new()"),
    ];
    list.sort();
    list.into_iter()
        .map(|(f, l)| (f.to_string(), l.to_string()))
        .collect()
}

#[test]
fn only_the_listed_places_touch_files_and_the_lock_is_only_looked_at() {
    assert_eq!(
        found(),
        expected(),
        "a line that touches a file or names db.lck was added, changed or removed: read it, then update this list"
    );
}

#[test]
fn nothing_but_a_look_ever_names_the_lock() {
    // Every line that names the lock path is one of four: its constant, the field, its initialisation, and the stat.
    let lock_lines: Vec<(String, String)> = found()
        .into_iter()
        .filter(|(_, line)| {
            line.contains("db.lck") || line.contains("DATABASE_LOCK") || line.contains("lock_path")
        })
        .collect();
    assert_eq!(lock_lines.len(), 4, "{lock_lines:?}");
    assert!(lock_lines.iter().all(|(file, _)| file == "runner.rs"));
    assert!(
        lock_lines
            .iter()
            .filter(|(_, line)| line.contains('('))
            .all(|(_, line)| line.contains("symlink_metadata") || line.contains("PathBuf::from")),
        "{lock_lines:?}"
    );
}
