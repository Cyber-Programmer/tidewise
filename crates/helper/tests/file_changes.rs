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
const WORDS: [&str; 23] = [
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
    // lists.rs (6.10b): its names for the real lock and the private one.
    "real_lock",
    "REAL_LOCK",
    "LOCK_NAME",
    // ... and the calls that read, write, size or flush a file without a name from the list above (review 6.10b R-06).
    "io::copy",
    ".truncate(",
    "File::open",
    "sync_all",
    "set_modified",
    // The one `flock` (6.10b R-01): on the private folder, never on a file of pacman's.
    "flock",
];

/// The files `lib.rs` declares under `#[cfg(test)]` (today `testing.rs`): never compiled into the real helper, so
/// they are not production code and are not read here.
fn test_only_files(src: &Path) -> Vec<String> {
    let lib = fs::read_to_string(src.join("lib.rs")).unwrap();
    let mut files = Vec::new();
    let mut lines = lib.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if line != "#[cfg(test)]" {
            continue;
        }
        let declaration = lines.next().unwrap_or_default();
        let declaration = declaration
            .trim_start_matches("pub(crate) ")
            .trim_start_matches("pub ");
        if let Some(name) = declaration
            .strip_prefix("mod ")
            .and_then(|rest| rest.strip_suffix(';'))
        {
            files.push(format!("{name}.rs"));
        }
    }
    files
}

/// Every such line of the non-test code: (file, line without its indentation).
fn found() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let skipped = test_only_files(&src);
    let mut found = Vec::new();
    for entry in fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".rs") || skipped.contains(&name) {
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
        // The private copy of the package lists (6.10b, decision 0007, lists.rs). Writes: the private folder and its
        // `sync/` (mkdir 0711, then looked at again), the `local` link, the staged copy `.tidewise-<name>.new` in the real
        // sync folder (O_NOFOLLOW, truncating, mode 0644) and the ONE rename of a staged list over the real one.
        // Nothing is deleted. The locks (real and private) appear only as constants, struct fields and arguments of
        // `looks_present`, which is a `symlink_metadata`.
        (
            "lists.rs",
            "use std::fs::{self, DirBuilder, File, OpenOptions};",
        ),
        (
            "lists.rs",
            "use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink};",
        ),
        (
            "lists.rs",
            "match DirBuilder::new().mode(0o711).create(path) {",
        ),
        ("lists.rs", "match symlink(&self.local_target, &link) {"),
        (
            "lists.rs",
            "if meta.file_type().is_symlink() && meta.uid() == self.owner && right_target {",
        ),
        ("lists.rs", "let mut source = OpenOptions::new()"),
        // The private folder opened (no link followed, close-on-exec by std) and locked with an exclusive flock, so one
        // helper at a time works in the private copy (review 6.10b R-01). It creates no file and is not `db.lck`.
        ("lists.rs", "let folder = OpenOptions::new()"),
        (
            "lists.rs",
            "match flock(&folder, FlockOperation::NonBlockingLockExclusive) {",
        ),
        (
            "lists.rs",
            "use rustix::fs::{FlockOperation, OFlags, flock};",
        ),
        // Read-only open of the folder to flush it, the truncating create of the staged copy, the copy, the list's own
        // time put on the copy, and the two flushes (6.10b review R-06 added these words to the guard).
        (
            "lists.rs",
            "let flushed = File::open(&self.real_sync).and_then(|folder| folder.sync_all());",
        ),
        ("lists.rs", ".truncate(true)"),
        (
            "lists.rs",
            "io::copy(&mut source, &mut staged).map_err(|_| ListsError::Io)?;",
        ),
        (
            "lists.rs",
            ".set_modified(meta.modified().map_err(|_| ListsError::Io)?)",
        ),
        (
            "lists.rs",
            "staged.sync_all().map_err(|_| ListsError::Io)?;",
        ),
        ("lists.rs", "let mut staged = OpenOptions::new()"),
        (
            "lists.rs",
            ".set_permissions(fs::Permissions::from_mode(0o644))",
        ),
        (
            "lists.rs",
            "fs::rename(dir.join(staged_name), dir.join(list_name))",
        ),
        (
            "lists.rs",
            "pub const REAL_LOCK: &str = \"/var/lib/pacman/db.lck\";",
        ),
        ("lists.rs", "const LOCK_NAME: &str = \"db.lck\";"),
        ("lists.rs", "real_lock: PathBuf,"),
        ("lists.rs", "real_lock: PathBuf::from(REAL_LOCK),"),
        ("lists.rs", "if looks_present(&self.real_lock) {"),
        ("lists.rs", "if looks_present(&self.real_lock) {"),
        ("lists.rs", "looks_present(&self.private.join(LOCK_NAME))"),
        // Read-only opens of the pacman output files the runner itself made in the work folder (reading stdout while it
        // grows, and the head of both files afterwards). Listed since 6.10b added `File::open` to the guard's words.
        (
            "runner.rs",
            "let mut stdout_reader = File::open(&files.out_path).map_err(|_| RunnerError::Io)?;",
        ),
        ("runner.rs", "if let Ok(file) = File::open(path) {"),
        // The one removal: a regular file directly in the work folder, after symlink_metadata (removal.rs).
        (
            "removal.rs",
            "fs::remove_file(&target).map_err(|_| RemovalError::Io)",
        ),
        // The one write outside the work folder, and the one on a /proc file: the helper's own oom_score_adj, set to 0 at
        // start (question 37). The path is a constant of env.rs, `/proc/self/...`, so it names no other file; the value is
        // read back and judged, never trusted.
        (
            "env.rs",
            "let _ = std::fs::write(OOM_SCORE_ADJ_PATH, \"0\");",
        ),
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
    // Every line that names a lock is one of eleven: in runner.rs its constant, the field, its initialisation and the stat;
    // in lists.rs (6.10b) the two constants, the real lock's field and initialisation, the two looks at the real lock and
    // the one at the private lock. Each look is a `symlink_metadata` (runner.rs directly, lists.rs through `looks_present`).
    let lock_lines: Vec<(String, String)> = found()
        .into_iter()
        .filter(|(_, line)| {
            [
                "db.lck",
                "DATABASE_LOCK",
                "lock_path",
                "real_lock",
                "REAL_LOCK",
                "LOCK_NAME",
            ]
            .iter()
            .any(|word| line.contains(word))
        })
        .collect();
    assert_eq!(lock_lines.len(), 11, "{lock_lines:?}");
    assert!(
        lock_lines
            .iter()
            .all(|(file, _)| file == "runner.rs" || file == "lists.rs")
    );
    assert!(
        lock_lines
            .iter()
            .filter(|(_, line)| line.contains('('))
            .all(|(_, line)| line.contains("symlink_metadata")
                || line.contains("PathBuf::from")
                || line.contains("looks_present")),
        "{lock_lines:?}"
    );
    // ... and `looks_present` itself looks with `symlink_metadata` and nothing else.
    let lists =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lists.rs")).unwrap();
    let body = lists
        .split("fn looks_present(")
        .nth(1)
        .and_then(|rest| rest.split("\n}\n").next())
        .unwrap();
    assert!(body.contains("fs::symlink_metadata(path)"), "{body}");
    assert!(
        !body.contains("File::") && !body.contains("OpenOptions") && !body.contains("fs::read")
    );
}
