//! The polkit policy and the real pkexec (WBS 5.4 and 5.4b; threats T-01, T-02, T-03). Real polkitd and real pkexec
//! inside the Tidewise test container only.
//!
//! What is checked here:
//!  1. the policy file is valid against polkit's own DTD (`xmllint --dtdvalid`), and a deliberately broken file is
//!     rejected by the same command (so the check can fail);
//!  2. polkitd, started on a private system bus, reads the file and lists the action with the agreed defaults
//!     (`pkaction --verbose`): implicit any `no`, inactive `no`, active `auth_admin`, the helper's absolute path, the
//!     message with `$(command_line)`. This is polkitd's own list of actions (`pkaction` asks it);
//!  3. (5.4b) the real `pkexec`, run as root (polkitd authorizes root with no agent), with our real policy and the
//!     helper copied to its installed path: what `argv[0]` pkexec gives a program (the name as typed; only a name
//!     found through `PATH` becomes absolute), the helper running a job when started under its own name, and the
//!     helper refusing a symlink's name (review R-02, threat model question 22).
//!
//! What is NOT checked here, and why:
//!  - the password dialog. What it shows was checked by hand on 15 Nov 2026 on a KDE agent, with a TEST action that
//!    has the same message but runs `/usr/bin/true` (decision 0005, threat T-02). Not checked by anyone: GNOME's
//!    agent, a hostile argv in a dialog (threat model, open question 21, backlog for 5.6).
//!  - which action pkexec picked. As root, pkexec is authorized whatever the action, and polkitd's log does not name
//!    it. That pkexec matches `exec.path` by the real path, exactly, is from polkit's source (`find_action_for_path`).
//!    So the pkexec tests below would pass just the same if pkexec had fallen back to the stock action: they show what
//!    `argv[0]` the helper receives and what it does with it, and must not be cited for the action match.
//!  - pkexec's authentication of an ordinary user, and `allow_active` (the container has no logind session, so
//!    "active" is never true here). The policy's text is what is checked.
//!  - the package that installs the helper at its path with the right owner: WBS 5.5. Here the test copies it.
//!
//! The policy comes into the container inside this test binary (`include_str!`), so no new host path is mounted; the
//! helper is the read-only binary run.sh already mounts, copied inside the container.
//! The tests are `#[ignore]`, so `cargo test --all` on the host skips them. Run them with `tests/container/run.sh`.
//! The system bus and polkitd are started by hand because the container has no systemd; they are stopped again
//! when each test ends.

mod common;

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::{
    HELPER, REPO, assert_inside_the_test_container, confirmed, feed, install_request,
    installed_version, reset_world,
};

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

// ---- the real pkexec (5.4b) -----------------------------------------------------------------------------

/// Where the policy says the helper is, and where the package puts it.
const INSTALLED_HELPER: &str = "/usr/lib/tidewise/tidewise-helper";
const REFUSED_NAME: &str = "tidewise-helper: it was not started under its own installed name, so it did not start. Nothing was changed.\n";

/// The helper copied to the path the policy and the package use. pkexec runs the real path of what it is given and
/// matches `exec.path` with it exactly, so a symlink to the read-only mount would not do. Removed again on drop.
struct InstalledHelper;

impl InstalledHelper {
    fn new() -> InstalledHelper {
        fs::create_dir_all("/usr/lib/tidewise").unwrap();
        fs::copy(HELPER, INSTALLED_HELPER).unwrap();
        fs::set_permissions(INSTALLED_HELPER, fs::Permissions::from_mode(0o755)).unwrap();
        InstalledHelper
    }
}

impl Drop for InstalledHelper {
    fn drop(&mut self) {
        let _ = fs::remove_file(INSTALLED_HELPER);
        let _ = fs::remove_dir("/usr/lib/tidewise");
    }
}

struct Piped {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// `pkexec <program> <args>` as root, with an empty environment (plus `PATH` when given), the request on stdin.
fn pkexec(program: &str, args: &[&str], path: Option<&str>, stdin: &[u8]) -> Piped {
    // Through `timeout`: the test binary is process 1 of the container, and pkexec refuses to serve a parent that is
    // process 1 ("Refusing to render service to dead parents", exit 127). `timeout` is a real parent in between, and
    // a limit in case something waits for an agent that is not there.
    let mut command = Command::new("/usr/bin/timeout");
    command
        .args(["60", "/usr/bin/pkexec"])
        .arg(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let mut child = command.spawn().expect("could not start pkexec");
    let writer = feed(&mut child, stdin);
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    Piped {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// An install of one dummy package and the summary for it, from the list `pacman --print` gives right now.
fn install_hello() -> (String, String) {
    let items = confirmed("-S", &[&format!("{REPO}/tidewise-dummy-hello")]);
    install_request(&[(REPO, "tidewise-dummy-hello")], &[], &items)
}

#[test]
#[ignore = "runs only in the test container: tests/container/run.sh"]
fn polkit_pkexec_gives_the_program_the_name_it_was_typed_with() {
    // The premise of the helper's argv[0] check (question 22). A test action for bash, so the program can show its
    // own argv: /proc/<pid>/cmdline.
    let policy = POLICY
        .replace(ACTION_ID, "io.github.tidewise.test-argv0")
        .replace(INSTALLED_HELPER, "/usr/bin/bash");
    assert_ne!(policy, POLICY, "the replacements did nothing");
    let _polkit = Polkit::start(&policy);
    let show = r"tr '\0' '|' < /proc/$$/cmdline";
    let first_word = |ran: &Piped| ran.stdout.split('|').next().unwrap_or_default().to_string();

    let direct = pkexec("/usr/bin/bash", &["-c", show], None, b"");
    assert_eq!(direct.code, Some(0), "{} {}", direct.stdout, direct.stderr);
    assert_eq!(first_word(&direct), "/usr/bin/bash");

    // A symlink to bash, in a folder named like a sentence: pkexec matches by the real path (so our action would
    // apply), and the program is told the name it was typed with.
    let folder = "/tmp/NO PASSWORD NEEDED, press Cancel";
    fs::create_dir_all(folder).unwrap();
    let link = format!("{folder}/bash");
    symlink("/usr/bin/bash", &link).unwrap();
    let through_link = pkexec(&link, &["-c", show], None, b"");
    assert_eq!(
        through_link.code,
        Some(0),
        "{} {}",
        through_link.stdout,
        through_link.stderr
    );
    assert_eq!(first_word(&through_link), link);

    // Only a name found through PATH becomes absolute.
    let relative = pkexec("bash", &["-c", show], Some("/usr/bin"), b"");
    assert_eq!(
        relative.code,
        Some(0),
        "{} {}",
        relative.stdout,
        relative.stderr
    );
    assert_eq!(first_word(&relative), "/usr/bin/bash");
    let _ = fs::remove_file(&link);
    let _ = fs::remove_dir(folder);
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn polkit_a_real_pkexec_start_at_the_installed_path_runs_the_job() {
    assert_inside_the_test_container();
    reset_world();
    let (request, summary) = install_hello();
    let _helper = InstalledHelper::new();
    let _polkit = Polkit::start(POLICY);

    let ran = pkexec(INSTALLED_HELPER, &[&summary], None, request.as_bytes());
    assert_eq!(ran.code, Some(0), "{}\n{}", ran.stdout, ran.stderr);
    assert_eq!(
        ran.stdout.lines().last(),
        Some("{\"finished\":\"ok\"}"),
        "{}",
        ran.stdout
    );
    assert_eq!(ran.stderr, "");
    assert!(installed_version("tidewise-dummy-hello").is_some());
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn polkit_a_symlink_to_the_helper_is_run_by_pkexec_and_refused_by_the_helper() {
    // Review R-02: any process of the user can start our action through a symlink with words in its name, and
    // pkexec would show those words in the dialog. The helper must not run the job.
    assert_inside_the_test_container();
    reset_world();
    let (request, summary) = install_hello();
    let _helper = InstalledHelper::new();
    let _polkit = Polkit::start(POLICY);
    let folder = "/tmp/NO PASSWORD NEEDED, press Cancel";
    fs::create_dir_all(folder).unwrap();
    let link = format!("{folder}/tidewise-helper");
    symlink(INSTALLED_HELPER, &link).unwrap();

    let ran = pkexec(&link, &[&summary], None, request.as_bytes());
    // The helper ran (pkexec found our action by the real path) and refused to start: exit 2, one fixed sentence.
    assert_eq!(ran.code, Some(2), "{}\n{}", ran.stdout, ran.stderr);
    assert_eq!(ran.stdout, "");
    assert_eq!(ran.stderr, REFUSED_NAME);
    assert_eq!(installed_version("tidewise-dummy-hello"), None);
    let _ = fs::remove_file(&link);
    let _ = fs::remove_dir(folder);
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn polkit_a_name_found_through_path_arrives_as_the_installed_path_and_is_accepted() {
    // The helper's own name, looked up through PATH, reaches it as the absolute path. A wrong summary is a cheap probe
    // that the start checks passed: the request is then refused for its summary, not at the start.
    assert_inside_the_test_container();
    reset_world();
    let (request, _summary) = install_hello();
    let _helper = InstalledHelper::new();
    let _polkit = Polkit::start(POLICY);

    let ran = pkexec(
        "tidewise-helper",
        &["not the summary"],
        Some("/usr/lib/tidewise"),
        request.as_bytes(),
    );
    assert_eq!(ran.code, Some(1), "{}\n{}", ran.stdout, ran.stderr);
    let last = ran.stdout.lines().last().unwrap_or_default();
    assert!(last.contains("summary_mismatch"), "{last}");
    assert_eq!(ran.stderr, "");
}
