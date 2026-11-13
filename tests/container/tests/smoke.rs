//! Smoke tests: prove the harness works. Real pacman, as root, inside the Tidewise test container only.
//!
//! They are `#[ignore]`, so `cargo test --all` on the host never runs them. Run them with `tests/container/run.sh`.
//! Run directly on the host they refuse (the guard), which is the point.
//!
//! The container's package lists were refreshed when the image was built, so these tests only ever run `pacman -S`
//! against the current lists, never `-Sy` (golden rule 4). `--noconfirm` is fine here: fixed names, throwaway
//! container, no user input.

use std::process::{Command, Output, Stdio};

use tidewise_container_tests::require_container;

/// Same way `crates/pacman/src/run.rs` runs pacman: absolute path, argument list, empty environment except
/// `LC_ALL=C`, no stdin, no shell.
fn pacman(args: &[&str]) -> Output {
    Command::new("/usr/bin/pacman")
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .expect("could not start /usr/bin/pacman")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Every container test starts here. It asserts (rather than panics inside the library) so the refusal reads plainly.
fn assert_inside_the_test_container() {
    let guard = require_container();
    assert!(
        guard.is_ok(),
        "refusing to run: {}",
        guard.err().map(|why| why.to_string()).unwrap_or_default()
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn pacman_installs_a_dummy_package_from_the_local_repo() {
    assert_inside_the_test_container();

    let install = pacman(&["-S", "--noconfirm", "tidewise-dummy-hello"]);
    assert!(
        install.status.success(),
        "pacman -S failed ({:?}): {}",
        install.status.code(),
        text(&install.stderr)
    );

    let query = pacman(&["-Q", "tidewise-dummy-hello"]);
    assert!(query.status.success(), "{}", text(&query.stderr));
    assert_eq!(text(&query.stdout), "tidewise-dummy-hello 1.0-1\n");
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn pacman_installs_the_dependency_together_with_the_package() {
    assert_inside_the_test_container();

    // tidewise-dummy-app depends on tidewise-dummy-lib (which nothing else in these tests installs).
    let install = pacman(&["-S", "--noconfirm", "tidewise-dummy-app"]);
    assert!(
        install.status.success(),
        "pacman -S failed ({:?}): {}",
        install.status.code(),
        text(&install.stderr)
    );

    let query = pacman(&["-Q", "tidewise-dummy-app", "tidewise-dummy-lib"]);
    assert!(query.status.success(), "{}", text(&query.stderr));
    assert_eq!(
        text(&query.stdout),
        "tidewise-dummy-app 1.0-1\ntidewise-dummy-lib 1.0-1\n"
    );
}
