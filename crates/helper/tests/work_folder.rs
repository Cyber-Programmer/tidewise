//! Production code can only ever work in the real work folder, `/run/tidewise-helper` (WBS 5.4; threats T-03, T-17,
//! T-20; golden rule 7). A `WorkDir` is the proof that a folder is the verified one (a real directory, owned by root,
//! mode 0700), and the only code that deletes files in the helper (`removal.rs`) accepts only paths inside it.
//! So `WorkDir` must not be buildable for any other folder outside the tests.
//!
//! Two defences:
//!  - at compile time: the constructor for an arbitrary folder, `WorkDir::open`, exists only under `#[cfg(test)]`.
//!    The doctests on `WorkDir` prove it (`compile_fail`), next to one that compiles with `open_default`;
//!  - here, in the source: no production line calls `WorkDir::open`, `main.rs` uses `open_default`, exactly one
//!    production line builds a `WorkDir` value, and the test-only constructor is the only thing between the
//!    production code and the unit tests in `workdir.rs`.
//!
//! Like `file_changes.rs` this reads each source file up to its first `#[cfg(test)]`. That is why the test-only
//! constructor has to sit right before `mod tests`: anything placed after a `#[cfg(test)]` marker would not be read.
//! Two more checks keep that blind spot closed for both guards (review R-04, 5.4b): `src/` has no subfolders, and
//! after its first marker a file holds only unit tests (a production item there would start in column 0, which
//! rustfmt, run in CI, does not allow for anything inside a block).
//! This is a prompt for review, not a proof of absence.

use std::fs;
use std::path::Path;

fn src_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The files `lib.rs` declares under `#[cfg(test)]` (today `testing.rs`): never compiled into the real helper, so
/// they are not production code and are not read here.
fn test_only_files() -> Vec<String> {
    let lib = fs::read_to_string(src_dir().join("lib.rs")).unwrap();
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

/// Every `.rs` file in `src/` that is compiled into the real helper: (name, whole text).
fn sources() -> Vec<(String, String)> {
    let skipped = test_only_files();
    let mut found = Vec::new();
    for entry in fs::read_dir(src_dir()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".rs") && !skipped.contains(&name) {
            found.push((name, fs::read_to_string(&path).unwrap()));
        }
    }
    found.sort();
    found
}

/// The production lines of every file: (file, line without indentation), comments left out.
fn production_lines() -> Vec<(String, String)> {
    let mut lines = Vec::new();
    for (name, text) in sources() {
        let code = text.split("#[cfg(test)]").next().unwrap();
        for line in code.lines() {
            let line = line.trim();
            if !line.starts_with("//") {
                lines.push((name.clone(), line.to_string()));
            }
        }
    }
    lines
}

fn text_of(file: &str) -> String {
    fs::read_to_string(src_dir().join(file)).unwrap()
}

#[test]
fn no_production_code_calls_workdir_open() {
    let calls: Vec<_> = production_lines()
        .into_iter()
        .filter(|(_, line)| line.contains("WorkDir::open("))
        .collect();
    assert!(
        calls.is_empty(),
        "production code calls WorkDir::open: {calls:?}"
    );
}

#[test]
fn main_opens_the_fixed_work_folder() {
    let main = text_of("main.rs");
    let code = main.split("#[cfg(test)]").next().unwrap();
    assert!(
        code.contains("WorkDir::open_default()"),
        "main.rs must open the work folder with WorkDir::open_default()"
    );
}

#[test]
fn exactly_one_production_line_builds_a_workdir() {
    let builders: Vec<_> = production_lines()
        .into_iter()
        .filter(|(_, line)| {
            line.contains("WorkDir {")
                && !line.starts_with("pub struct WorkDir {")
                && !line.starts_with("impl WorkDir {")
        })
        .collect();
    assert_eq!(builders.len(), 1, "{builders:?}");
    assert_eq!(builders[0].0, "workdir.rs", "{builders:?}");
}

#[test]
fn the_test_only_constructor_sits_alone_before_the_unit_tests() {
    let text = text_of("workdir.rs");
    let pieces: Vec<&str> = text.split("#[cfg(test)]").collect();
    assert_eq!(
        pieces.len(),
        3,
        "workdir.rs must have exactly two #[cfg(test)] items: the test-only constructor, then `mod tests`"
    );
    let constructor = pieces[1];
    assert!(constructor.trim_start().starts_with("impl WorkDir {"));
    assert!(constructor.contains("pub(crate) fn open("));
    assert_eq!(
        constructor.matches("fn ").count(),
        1,
        "nothing but the constructor may sit there"
    );
    assert!(pieces[2].trim_start().starts_with("mod tests {"));
}

#[test]
fn src_has_no_subfolders() {
    // The guards read `src/*.rs` only (review R-04): a module in a subfolder would not be read.
    let folders: Vec<_> = fs::read_dir(src_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    assert!(folders.is_empty(), "subfolders of src/: {folders:?}");
}

#[test]
fn after_its_first_test_marker_a_file_holds_only_unit_tests() {
    // The guards read each file only up to its first `#[cfg(test)]`. So what follows must be unit tests and nothing
    // else (review R-04). rustfmt indents everything inside a block, so a production item placed after `mod tests`
    // would start in column 0: here only the closing brace, the test module, the test-only constructor of WorkDir,
    // comments, further markers and (in lib.rs) module declarations may.
    for (name, text) in sources() {
        let Some((_, after)) = text.split_once("#[cfg(test)]") else {
            continue;
        };
        for line in after.lines() {
            let is_module_declaration = name == "lib.rs"
                && line.ends_with(';')
                && (line.starts_with("pub mod ")
                    || line.starts_with("pub(crate) mod ")
                    || line.starts_with("mod "));
            let allowed = line.is_empty()
                || line.starts_with(' ')
                || line.starts_with("//")
                || line.starts_with("#[cfg(test)]")
                || line == "}"
                || line == "mod tests {"
                || (name == "workdir.rs" && line == "impl WorkDir {")
                || is_module_declaration;
            assert!(
                allowed,
                "{name}: production code after the first #[cfg(test)], where the guards do not read: {line}"
            );
        }
    }
}
