//! The polkit policy (WBS 5.4; threats T-01, T-02, T-03). Real polkitd inside the Tidewise test container only.
//!
//! What is checked here:
//!  1. the policy file is valid against polkit's own DTD (`xmllint --dtdvalid`), and a deliberately broken file is
//!     rejected by the same command (so the check can fail);
//!  2. polkitd, started on a private system bus, reads the file and lists the action with the agreed defaults
//!     (`pkaction --verbose`): implicit any `no`, inactive `no`, active `auth_admin`, the helper's absolute path, the
//!     message with `$(command_line)`. pkexec learns about actions from the same list (EnumerateActions).
//!
//! What is NOT checked here, and why:
//!  - pkexec itself. It is setuid, and the container runs with `no-new-privileges`, so it cannot become root; and an
//!    authentication needs an agent. So `find_action_for_path` (the exact comparison with `exec.path`) and the dialog
//!    are not run. The dialog was checked by hand on a KDE agent (decision 0005, threat T-02).
//!  - `allow_active`: the container has no logind session, so "active" is never true here. The file's text is what
//!    is checked.
//!  - the helper at its real path (`/usr/lib/tidewise/tidewise-helper`): the package, WBS 5.5.
//!
//! The policy comes into the container inside this test binary (`include_str!`), so no new host path is mounted.
//! The tests are `#[ignore]`, so `cargo test --all` on the host skips them. Run them with `tests/container/run.sh`.
//! The system bus and polkitd are started by hand because the container has no systemd; they are stopped again
//! when each test ends.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::assert_inside_the_test_container;

/// The policy as it is in the repository, embedded at build time.
const POLICY: &str = include_str!("../../../packaging/polkit/io.github.tidewise.helper.policy");

const ACTION_ID: &str = "io.github.tidewise.helper";
const ACTIONS_DIR: &str = "/usr/share/polkit-1/actions";
const POLICY_PATH: &str = "/usr/share/polkit-1/actions/io.github.tidewise.helper.policy";
const DTD: &str = "/usr/share/polkit-1/policyconfig-1.dtd";

struct Ran {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Runs a program as pkaction and xmllint are meant to be run: absolute path, argument list, empty environment except
/// `LC_ALL=C`, no stdin, no shell.
fn run(program: &str, args: &[&str]) -> Ran {
    let output = Command::new(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("could not start {program}: {error}"));
    Ran {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `xmllint` against polkit's DTD. `--nonet`: nothing is fetched; the DTD is the local file from the polkit package.
fn dtd_check(file: &str) -> Ran {
    run(
        "/usr/bin/xmllint",
        &["--noout", "--nonet", "--dtdvalid", DTD, file],
    )
}

/// A system bus and a polkitd with the policy installed. Everything is stopped, and the file removed, on drop.
struct Polkit {
    bus: Child,
    polkitd: Child,
}

impl Polkit {
    /// Installs `policy`, starts the bus and polkitd, and waits until pkaction can list actions.
    fn start(policy: &str) -> Polkit {
        assert_inside_the_test_container();
        fs::write(POLICY_PATH, policy).unwrap();

        fs::create_dir_all("/run/dbus").unwrap();
        let bus = Command::new("/usr/bin/dbus-daemon")
            .args(["--system", "--nofork", "--nopidfile"])
            .env_clear()
            .stdin(Stdio::null())
            .spawn()
            .expect("could not start dbus-daemon");
        wait_for(|| Path::new("/run/dbus/system_bus_socket").exists());

        let polkitd = Command::new("/usr/lib/polkit-1/polkitd")
            .arg("--no-debug")
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .spawn()
            .expect("could not start polkitd");
        let started = Polkit { bus, polkitd };
        // Listing every action succeeds only when polkitd has taken its name on the bus.
        wait_for(|| {
            let listing = run("/usr/bin/pkaction", &[]);
            listing.success && !listing.stdout.is_empty()
        });
        started
    }
}

impl Drop for Polkit {
    fn drop(&mut self) {
        let _ = self.polkitd.kill();
        let _ = self.polkitd.wait();
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = fs::remove_file(POLICY_PATH);
        let _ = fs::remove_file("/run/dbus/system_bus_socket");
    }
}

/// Polls `ready` for up to 15 seconds.
fn wait_for(ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready() {
        assert!(Instant::now() < deadline, "not ready after 15 seconds");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The value after `label` in a `pkaction --verbose` line such as `  implicit active:   auth_admin`.
fn field<'a>(listing: &'a str, label: &str) -> Option<&'a str> {
    listing
        .lines()
        .find_map(|line| line.trim_start().strip_prefix(label))
        .map(str::trim)
}

#[test]
#[ignore = "runs only in the test container: tests/container/run.sh"]
fn the_policy_is_valid_against_polkits_dtd() {
    assert_inside_the_test_container();
    assert!(
        Path::new(ACTIONS_DIR).is_dir() && Path::new(DTD).is_file(),
        "the image has no polkit"
    );
    fs::write(POLICY_PATH, POLICY).unwrap();
    let checked = dtd_check(POLICY_PATH);
    let _ = fs::remove_file(POLICY_PATH);
    assert!(checked.success, "xmllint: {}", checked.stderr);
    // The DOCTYPE's web address is not fetched (--nonet), which xmllint reports as an I/O warning. A validity problem
    // would be a different message.
    assert!(
        !checked.stderr.contains("validity error") && !checked.stderr.contains("does not validate"),
        "xmllint: {}",
        checked.stderr
    );
}

#[test]
#[ignore = "runs only in the test container: tests/container/run.sh"]
fn a_broken_policy_fails_the_same_polkit_check() {
    assert_inside_the_test_container();
    // Valid XML, but not what the DTD allows: the action has no <defaults>, and it has an element the DTD lacks.
    let broken = POLICY
        .replace("<defaults>", "<allow_always>")
        .replace("</defaults>", "</allow_always>");
    assert_ne!(broken, POLICY, "the replacement did nothing");
    let path = "/tmp/tidewise-broken.policy";
    fs::write(path, &broken).unwrap();
    let checked = dtd_check(path);
    let _ = fs::remove_file(path);
    assert!(!checked.success, "xmllint accepted a broken policy");
    assert!(
        checked.stderr.contains("validity error") || checked.stderr.contains("does not validate"),
        "xmllint: {}",
        checked.stderr
    );
}

#[test]
#[ignore = "runs only in the test container: tests/container/run.sh"]
fn polkitd_lists_the_action_with_the_agreed_defaults() {
    let _polkit = Polkit::start(POLICY);
    let shown = run(
        "/usr/bin/pkaction",
        &["--action-id", ACTION_ID, "--verbose"],
    );
    assert!(shown.success, "pkaction: {} {}", shown.stdout, shown.stderr);
    let text = shown.stdout;

    assert_eq!(field(&text, "implicit any:"), Some("no"), "{text}");
    assert_eq!(field(&text, "implicit inactive:"), Some("no"), "{text}");
    assert_eq!(
        field(&text, "implicit active:"),
        Some("auth_admin"),
        "{text}"
    );
    assert!(
        text.contains("org.freedesktop.policykit.exec.path -> /usr/lib/tidewise/tidewise-helper"),
        "{text}"
    );
    let message = field(&text, "message:").unwrap_or_default();
    assert!(message.contains("$(command_line)"), "{text}");
    assert!(!text.contains("allow_gui"), "{text}");
    assert!(!text.contains("exec.argv1"), "{text}");
}

#[test]
#[ignore = "runs only in the test container: tests/container/run.sh"]
fn polkitd_does_not_list_an_action_that_is_not_installed() {
    // Control for the test above: pkaction finds only an action polkitd really has. The same file is installed under
    // another action id, so the helper's id must be unknown while the other one is found.
    let _polkit = Polkit::start(&POLICY.replace(ACTION_ID, "io.github.tidewise.other"));
    let missing = run("/usr/bin/pkaction", &["--action-id", ACTION_ID]);
    assert!(!missing.success, "the helper's action should not exist");
    assert!(
        missing.stdout.contains("No action with action id")
            || missing.stderr.contains("No action with action id"),
        "{} {}",
        missing.stdout,
        missing.stderr
    );
    let other = run(
        "/usr/bin/pkaction",
        &["--action-id", "io.github.tidewise.other"],
    );
    assert!(other.success, "{} {}", other.stdout, other.stderr);
}
