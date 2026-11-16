//! The package recipe as a test (WBS 5.5; threats T-03 and T-01, golden rules 1 and 2, decision 0005).
//!
//! `packaging/aur/PKGBUILD` is the recipe that puts the app, the root helper and the polkit policy on a user's disk.
//! This file reads it as plain text (no shell is run, no crate is added) and ties it to the code:
//!  - the helper lands at exactly `HELPER_PATH`, mode 0755, never setuid, and the recipe can do nothing else to a
//!    file (no `chmod`, no `chown`, no `cp`, no `ln`): `package()` is only `install` lines;
//!  - the policy goes to polkit's actions folder, from `packaging/polkit/`, mode 0644;
//!  - `pkgver` is the workspace version, `.SRCINFO` says the same as the PKGBUILD;
//!  - the build follows the Rust package guidelines, and nothing in the recipe asks for an rpath;
//!  - the release guard: `SKIP` as a checksum is allowed only for the local `git archive` tarball, and never in a
//!    release build (the real tagged-release source and its checksum come with v0.1, M4).
//!
//! That the built package really has these owners and modes, and installs and removes cleanly, is checked in the
//! container (`tests/container/tests/package.rs`).
//!
//! Sources, read from the local `arch-wiki-docs` 20261101-1 (the live wiki blocks bots): "Rust package guidelines"
//! (only the Portuguese translation of 2020-06-23 is in that snapshot), "Arch package guidelines", "PKGBUILD",
//! ".SRCINFO", "Desktop entries".

use std::fs;
use std::path::{Path, PathBuf};

use tidewise_helper::args::HELPER_PATH;

const POLICY_NAME: &str = "io.github.tidewise.helper.policy";
const DESKTOP_NAME: &str = "io.github.tidewise.desktop";

fn packaging() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging")
}

fn read(path: &Path) -> String {
    assert!(path.is_file(), "missing file: {}", path.display());
    fs::read_to_string(path).unwrap()
}

fn pkgbuild() -> String {
    read(&packaging().join("aur/PKGBUILD"))
}

fn srcinfo() -> String {
    read(&packaging().join("aur/.SRCINFO"))
}

fn desktop_entry() -> String {
    read(&packaging().join("desktop").join(DESKTOP_NAME))
}

/// The lines that are code: no blank lines, no comment lines. Surrounding blanks are trimmed.
fn code_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// `name=value` at the start of a line: the value without its quotes. `None` if there is no such line.
fn scalar(text: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    code_lines(text)
        .iter()
        .find_map(|line| line.strip_prefix(&prefix).map(str::to_string))
        .map(|value| value.trim_matches(['\'', '"']).to_string())
}

/// `name=(a b c)` on one line: the words without their quotes. The recipe keeps every array on one line on purpose
/// (and no item has a space in it), so this small reader is enough; a multi-line array is a failure, not a guess.
fn array(text: &str, name: &str) -> Vec<String> {
    let prefix = format!("{name}=(");
    let line = code_lines(text)
        .into_iter()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("the PKGBUILD has no `{name}=(...)` line"));
    let inside = line[prefix.len()..]
        .strip_suffix(')')
        .unwrap_or_else(|| panic!("`{name}=(` must be one line ending in `)`: {line}"));
    inside
        .split_whitespace()
        .map(|word| word.trim_matches(['\'', '"']).to_string())
        .collect()
}

/// The code lines inside `name() {` ... `}` (the closing brace alone at the start of a line). A line that ends in
/// a backslash is joined with the next one, as the shell reads it, so a long `install` line may be wrapped.
fn function_body(text: &str, name: &str) -> Vec<String> {
    let header = format!("{name}() {{");
    let mut lines = text.lines();
    assert!(
        lines.any(|line| line == header),
        "the PKGBUILD has no `{header}`"
    );
    let body: Vec<&str> = lines.take_while(|line| *line != "}").collect();
    let joined = body.join("\n").replace("\\\n", " ");
    code_lines(&joined)
}

/// A value as `makepkg --printsrcinfo` writes it: `$pkgname` and `$pkgver` replaced by their values.
fn expand(value: &str, text: &str) -> String {
    let name = scalar(text, "pkgname").unwrap();
    let version = scalar(text, "pkgver").unwrap();
    value
        .replace("${pkgname}", &name)
        .replace("$pkgname", &name)
        .replace("${pkgver}", &version)
        .replace("$pkgver", &version)
}

/// Every value of `key = value` in a `.SRCINFO`, in order.
fn srcinfo_values(text: &str, key: &str) -> Vec<String> {
    let prefix = format!("{key} = ");
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(&prefix))
        .map(str::to_string)
        .collect()
}

/// One `install -Dm<mode> <source> <destination>` line of `package()`, kept as written.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Install {
    mode: String,
    source: String,
    destination: String,
}

/// Every line of `package()` except the first `cd`. A line that is not an `install -Dm<mode> <source> <destination>`
/// of exactly four words fails here: `package()` may do nothing else (no `chmod`, `chown`, `cp`, `ln`, `mkdir`).
fn installs(package_body: &[String]) -> Vec<Install> {
    package_body
        .iter()
        .filter(|line| line.as_str() != r#"cd "$pkgname-$pkgver""#)
        .map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            assert!(
                words.len() == 4 && words[0] == "install" && words[1].starts_with("-Dm"),
                "package() may only hold `install -Dm<mode> <source> <destination>` lines, found: {line}"
            );
            Install {
                mode: words[1]["-Dm".len()..].to_string(),
                source: words[2].to_string(),
                destination: words[3].to_string(),
            }
        })
        .collect()
}

fn expected_installs() -> Vec<Install> {
    let install = |mode: &str, source: String, destination: String| Install {
        mode: mode.to_string(),
        source,
        destination: format!("\"$pkgdir{destination}\""),
    };
    let mut expected = vec![
        install(
            "755",
            "target/release/tidewise".to_string(),
            "/usr/bin/tidewise".to_string(),
        ),
        // The helper's path comes from the constant the helper itself checks `argv[0]` against, not from a copy.
        install(
            "755",
            "target/release/tidewise-helper".to_string(),
            HELPER_PATH.to_string(),
        ),
        install(
            "644",
            format!("packaging/polkit/{POLICY_NAME}"),
            format!("/usr/share/polkit-1/actions/{POLICY_NAME}"),
        ),
        install(
            "644",
            format!("packaging/desktop/{DESKTOP_NAME}"),
            format!("/usr/share/applications/{DESKTOP_NAME}"),
        ),
        install(
            "644",
            "LICENSE".to_string(),
            "/usr/share/licenses/tidewise/LICENSE".to_string(),
        ),
    ];
    expected.sort();
    expected
}

// ---- what is installed, where, with which mode --------------------------------------------------------------

#[test]
fn the_package_installs_exactly_these_files_with_these_modes() {
    let body = function_body(&pkgbuild(), "package");
    let mut found = installs(&body);
    found.sort();
    assert_eq!(found, expected_installs());
}

#[test]
fn no_file_is_installed_with_a_setuid_setgid_sticky_or_writable_mode() {
    // The modes above are the only ones; this says why they are safe: three digits, nothing writable by group or
    // other, no leading special-bit digit (a four-digit mode such as 4755 would be setuid).
    for install in installs(&function_body(&pkgbuild(), "package")) {
        assert_eq!(install.mode.len(), 3, "{install:?}");
        for digit in install.mode.chars().skip(1) {
            let bits = digit.to_digit(8).expect("an octal digit");
            assert_eq!(bits & 0b010, 0, "group or other may write: {install:?}");
        }
    }
}

#[test]
fn every_destination_is_under_usr_and_quoted() {
    // Arch package guidelines, "Directories": /usr/bin, /usr/lib/pkg, /usr/share/...; a package holds nothing in
    // /run, /tmp, /srv, /home, ... and never /opt or /usr/local. "Quote variables which may contain spaces".
    for install in installs(&function_body(&pkgbuild(), "package")) {
        assert!(
            install.destination.starts_with("\"$pkgdir/usr/") && install.destination.ends_with('"'),
            "{install:?}"
        );
        assert!(!install.destination.contains("/usr/local"), "{install:?}");
    }
}

#[test]
fn the_installed_sources_exist_in_the_repository() {
    // `target/release/...` is made by build(); the rest must be files of the source tree (and so of `git archive`).
    let root = packaging().join("..");
    for install in installs(&function_body(&pkgbuild(), "package")) {
        if !install.source.starts_with("target/") {
            let path = root.join(&install.source);
            assert!(path.is_file(), "missing source file: {}", path.display());
        }
    }
}

#[test]
fn the_packaged_policy_names_the_helpers_installed_path() {
    // The policy file that the package installs says the same path as the one the helper insists on and the
    // package installs it at (`exec.path`, threat model Q23).
    let policy = read(&packaging().join("polkit").join(POLICY_NAME));
    let annotation = format!("org.freedesktop.policykit.exec.path\">{HELPER_PATH}</annotate>");
    assert!(policy.contains(&annotation), "{policy}");
}

#[test]
fn the_functions_run_no_command_that_changes_modes_owners_or_the_system() {
    // Only the function bodies: `depends=(pacman ...)` is a list of names, not a command.
    let text = pkgbuild();
    let code = ["build", "check", "package"]
        .iter()
        .flat_map(|name| function_body(&text, name))
        .collect::<Vec<_>>()
        .join("\n");
    for word in [
        "chmod", "chown", "chgrp", "setcap", "sudo", "curl", "wget", "pacman", "ln -", "mkdir",
        "cp ",
    ] {
        assert!(!code.contains(word), "the PKGBUILD uses `{word}`");
    }
}

#[test]
fn the_recipe_never_asks_for_an_rpath() {
    // T-03: an RPATH or RUNPATH that points into a user-writable folder would be a way to load code as root.
    // Checked without regard to case, in code and in flags; the container test reads the built ELF files too.
    let text = pkgbuild().to_lowercase();
    let code = code_lines(&text).join("\n");
    assert!(!code.contains("rpath"), "the PKGBUILD mentions an rpath");
    assert!(!code.contains("runpath"));
    assert!(
        !code.contains("rustflags"),
        "RUSTFLAGS is not set by this recipe"
    );
}

// ---- version, fields ----------------------------------------------------------------------------------------

#[test]
fn pkgver_is_the_workspace_version() {
    // How pkgver is set: written in the PKGBUILD and tied to `[workspace.package] version` by this test, so a
    // version bump without a PKGBUILD bump fails `cargo test`.
    assert_eq!(
        scalar(&pkgbuild(), "pkgver").as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(scalar(&pkgbuild(), "pkgrel").as_deref(), Some("1"));
}

#[test]
fn the_fields_say_what_the_guidelines_ask_for() {
    let text = pkgbuild();
    assert_eq!(scalar(&text, "pkgname").as_deref(), Some("tidewise"));
    // "Arch package guidelines": SPDX identifier; the same text as Cargo.toml's `license`.
    assert_eq!(array(&text, "license"), ["GPL-3.0-or-later"]);
    assert_eq!(array(&text, "arch"), ["x86_64"]);
    // pkgdesc: about 80 characters or fewer, and not self-referencing ("Tidewise is ...").
    let description = scalar(&text, "pkgdesc").expect("pkgdesc");
    assert!(
        !description.is_empty() && description.len() <= 80,
        "{description}"
    );
    assert!(
        !description.to_lowercase().contains("tidewise"),
        "{description}"
    );
    assert!(scalar(&text, "url").is_some_and(|url| url.starts_with("https://")));
    // "Try to keep the line length in the PKGBUILD file below ~100 characters."
    for line in text.lines() {
        assert!(line.chars().count() <= 100, "line too long: {line}");
    }
}

#[test]
fn depends_name_pacman_and_polkit_and_makedepends_the_compiler() {
    let text = pkgbuild();
    let depends = array(&text, "depends");
    for needed in ["pacman", "polkit"] {
        assert!(depends.iter().any(|name| name == needed), "{depends:?}");
    }
    assert!(
        array(&text, "makedepends")
            .iter()
            .any(|name| name == "cargo")
    );
    // A version rule or a repository in depends would be a guess at what pacman can resolve.
    assert!(depends.iter().all(|name| !name.contains(['<', '>', '='])));
}

#[test]
fn the_empty_debug_package_is_turned_off() {
    // The release profile has no debug info, so makepkg's `tidewise-debug` package would be empty (it printed
    // "No debugging symbols" for both binaries). Proper debug info and a debug package: an M10 idea.
    assert_eq!(array(&pkgbuild(), "options"), ["!debug"]);
}

// ---- how it is built ----------------------------------------------------------------------------------------

#[test]
fn build_and_check_follow_the_rust_package_guidelines() {
    // "Rust package guidelines": `cargo build --release --locked` and `cargo test --release --locked`
    // (`--locked` keeps Cargo.lock as it is: reproducible builds). Our build adds only `-p` choices after it.
    let text = pkgbuild();
    let build = function_body(&text, "build");
    assert!(
        build
            .iter()
            .any(|line| line.starts_with("cargo build --release --locked")),
        "{build:?}"
    );
    let build_line = build.iter().find(|l| l.starts_with("cargo build")).unwrap();
    for package in ["tidewise-app", "tidewise-helper"] {
        assert!(
            build_line.contains(&format!("-p {package}")),
            "{build_line}"
        );
    }
    let check = function_body(&text, "check");
    assert!(
        check
            .iter()
            .any(|line| line.starts_with("cargo test --release --locked")),
        "{check:?}"
    );
    // The source is unpacked into `tidewise-<pkgver>/` (the tarball's prefix); every function starts there.
    for name in ["build", "check", "package"] {
        assert_eq!(
            function_body(&text, name).first().map(String::as_str),
            Some(r#"cd "$pkgname-$pkgver""#),
            "{name}()"
        );
    }
}

// ---- .SRCINFO says what the PKGBUILD says ---------------------------------------------------------------

#[test]
fn srcinfo_agrees_with_the_pkgbuild() {
    // `makepkg --printsrcinfo > .SRCINFO` ("Changes to PKGBUILD metadata do not automatically propagate"); the
    // container test compares the file with what makepkg prints from the built recipe. This is the quick check.
    let text = pkgbuild();
    let info = srcinfo();
    let one = |key: &str| -> Vec<String> { vec![scalar(&text, key).unwrap()] };
    for key in ["pkgdesc", "pkgver", "pkgrel", "url"] {
        assert_eq!(srcinfo_values(&info, key), one(key), "{key}");
    }
    for key in [
        "license",
        "arch",
        "depends",
        "makedepends",
        "options",
        "source",
        "sha256sums",
    ] {
        let expected: Vec<String> = array(&text, key)
            .iter()
            .map(|value| expand(value, &text))
            .collect();
        assert_eq!(srcinfo_values(&info, key), expected, "{key}");
    }
    assert_eq!(srcinfo_values(&info, "pkgbase"), ["tidewise"]);
    assert_eq!(srcinfo_values(&info, "pkgname"), ["tidewise"]);
}

// ---- the release guard (SKIP) ---------------------------------------------------------------------------

/// What is wrong with the checksums, if anything. `SKIP` is how the local `git archive` tarball is built until
/// the real release source exists (M4); it must never reach a downloaded source or a release.
fn skip_problem(source: &[String], sums: &[String], release_build: bool) -> Option<&'static str> {
    if source.len() != sums.len() {
        return Some("source and sha256sums have different lengths");
    }
    for (item, sum) in source.iter().zip(sums) {
        if sum != "SKIP" {
            continue;
        }
        if item.contains("://") {
            return Some("a downloaded source has the checksum SKIP");
        }
        if release_build {
            return Some("a release build has the checksum SKIP");
        }
    }
    None
}

fn words(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

#[test]
fn skip_is_allowed_for_a_local_tarball_outside_a_release() {
    let source = words(&["tidewise-0.1.0.tar.gz"]);
    assert_eq!(skip_problem(&source, &words(&["SKIP"]), false), None);
}

#[test]
fn skip_is_refused_for_a_downloaded_source() {
    let source = words(&["tidewise-0.1.0.tar.gz::https://example.org/x.tar.gz"]);
    assert!(skip_problem(&source, &words(&["SKIP"]), false).is_some());
    assert!(
        skip_problem(
            &words(&["git+https://example.org/x.git"]),
            &words(&["SKIP"]),
            false
        )
        .is_some()
    );
}

#[test]
fn skip_is_refused_in_a_release_build() {
    let source = words(&["tidewise-0.1.0.tar.gz"]);
    assert!(skip_problem(&source, &words(&["SKIP"]), true).is_some());
}

#[test]
fn a_real_checksum_passes_everywhere_and_a_missing_one_does_not() {
    let source = words(&["https://example.org/x.tar.gz"]);
    let real = words(&["a".repeat(64).as_str()]);
    assert_eq!(skip_problem(&source, &real, true), None);
    assert!(skip_problem(&source, &[], false).is_some());
}

#[test]
fn the_pkgbuild_passes_the_release_guard() {
    // In a release build (`TIDEWISE_RELEASE=1`, or a GitHub tag build) this fails while any `SKIP` is left: the
    // release blocker of 5.5. CI does not run on tags yet; that is an M4 item (threat model, release blocker).
    let release = std::env::var("TIDEWISE_RELEASE").is_ok_and(|value| value == "1")
        || std::env::var("GITHUB_REF_TYPE").is_ok_and(|value| value == "tag");
    let text = pkgbuild();
    let problem = skip_problem(
        &array(&text, "source"),
        &array(&text, "sha256sums"),
        release,
    );
    assert_eq!(problem, None);
}

// ---- the desktop entry ----------------------------------------------------------------------------------

#[test]
fn the_desktop_entry_starts_the_app_by_its_binary_name() {
    // "Desktop entries": Type and Name are required; an application also needs Exec. The binary is /usr/bin/tidewise
    // (the first install line of the recipe), so `Exec=tidewise` finds it through PATH.
    let entry = desktop_entry();
    assert_eq!(entry.lines().next(), Some("[Desktop Entry]"));
    let value = |key: &str| -> Option<String> {
        entry
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(str::to_string)
    };
    assert_eq!(value("Type").as_deref(), Some("Application"));
    assert_eq!(value("Name").as_deref(), Some("Tidewise"));
    assert_eq!(value("Exec").as_deref(), Some("tidewise"));
    assert_eq!(value("Icon").as_deref(), Some("system-software-install"));
    assert_eq!(value("Terminal").as_deref(), Some("false"));
    assert_eq!(value("Categories").as_deref(), Some("System;"));
    // The app is never started with more than its own name: no root, no shell, no arguments from outside.
    assert!(!entry.contains("pkexec") && !entry.contains("sudo") && !entry.contains("sh -c"));
}
