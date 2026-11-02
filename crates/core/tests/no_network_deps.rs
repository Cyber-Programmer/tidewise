//! Golden rule 6 for scan data: `tidewise-core` and `tidewise-adapters` must not depend, even
//! indirectly, on a networking crate. Checked with `cargo tree` over normal and build dependencies
//! (what ships); dev-dependencies are not part of the app.

use std::path::{Path, PathBuf};
use std::process::Command;

const NETWORK_CRATES: &[&str] = &[
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "curl-sys",
    "isahc",
    "attohttpc",
    "rustls",
    "native-tls",
    "openssl",
    "openssl-sys",
    "h2",
    "surf",
    "minreq",
    "tungstenite",
];

/// Names of listed networking crates in `package`'s dependency tree, sorted, no duplicates.
/// Panics if cargo fails, so an unknown package or a broken manifest can never read as "clean".
fn network_crates_in_tree(manifest: &Path, package: &str) -> Vec<String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "tree",
            "--offline",
            "--prefix",
            "none",
            "--edges",
            "normal,build",
        ])
        .args(["--package", package, "--manifest-path"])
        .arg(manifest)
        .env("LC_ALL", "C")
        .output()
        .expect("run cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed for {package}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.lines().next().is_some_and(|l| l.starts_with(package)),
        "cargo tree printed nothing for {package}: {text}"
    );
    // Lines look like `reqwest v0.12.4` or `reqwest v0.12.4 (/path) (*)`.
    let mut found: Vec<String> = text
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| NETWORK_CRATES.contains(name))
        .map(str::to_string)
        .collect();
    found.sort();
    found.dedup();
    found
}

fn real_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml")
}

/// Copies the fixture workspace to a temp dir so cargo writes its lock file there.
fn fixture_copy() -> (tempfile::TempDir, PathBuf) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/netdeps");
    let dir = tempfile::tempdir().expect("temp dir");
    let status = Command::new("cp")
        .arg("-r")
        .arg("--no-target-directory")
        .arg(&src)
        .arg(dir.path())
        .status()
        .expect("run cp");
    assert!(status.success(), "copying the fixture failed");
    let manifest = dir.path().join("Cargo.toml");
    (dir, manifest)
}

#[test]
fn fixture_with_a_transitive_networking_crate_is_caught() {
    let (_dir, manifest) = fixture_copy();
    assert_eq!(network_crates_in_tree(&manifest, "leaky"), vec!["reqwest"]);
}

#[test]
fn fixture_without_networking_crates_is_clean() {
    let (_dir, manifest) = fixture_copy();
    assert!(network_crates_in_tree(&manifest, "clean").is_empty());
}

#[test]
fn a_package_that_does_not_exist_is_an_error_not_a_pass() {
    let (_dir, manifest) = fixture_copy();
    let result = std::panic::catch_unwind(|| network_crates_in_tree(&manifest, "no-such-package"));
    assert!(
        result.is_err(),
        "an unknown package must not report 'clean'"
    );
}

#[test]
fn tidewise_core_has_no_networking_dependency() {
    assert!(network_crates_in_tree(&real_manifest(), "tidewise-core").is_empty());
}

#[test]
fn tidewise_adapters_has_no_networking_dependency() {
    assert!(network_crates_in_tree(&real_manifest(), "tidewise-adapters").is_empty());
}
