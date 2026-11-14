//! The real `tidewise-helper`, started as real root inside the Tidewise test container (WBS 5.3a).
//!
//! In 5.3a the helper has no jobs: a valid request ends with a fixed "not implemented" line. So these tests check the
//! gate end to end (exit codes, the one line on stdout, the one journal entry, hostile input, hostile environment,
//! inherited descriptors, a stdout or stderr that cannot be written) and that **nothing on the system changed**.
//!
//! They are `#[ignore]` and start with the container guard, like the smoke tests: run them with
//! `tests/container/run.sh`. On the host they refuse. A fake journald (a datagram socket bound to
//! `/run/systemd/journal/socket`, which does not exist in the image) receives what the helper logs.
//!
//! Not tested here, on purpose: `LD_PRELOAD` / `LD_LIBRARY_PATH`. The dynamic loader acts before `main`, so nothing
//! the helper does can defend against them; through `pkexec` they do not arrive (`pkexec(1)`: "a minimal known and
//! safe environment"). pkexec and polkit are not in the container, so this is only checked on a real machine (5.4).

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tidewise_container_tests::require_container;

const HELPER: &str = "/opt/tidewise/tidewise-helper";
const JOURNAL_DIR: &str = "/run/systemd/journal";
const JOURNAL_SOCKET: &str = "/run/systemd/journal/socket";

const INSTALL: &str = include_str!("../../fixtures/helper-protocol/install.json");
const UPDATE: &str = include_str!("../../fixtures/helper-protocol/update-system.json");
const BOTH: &str = include_str!("../../fixtures/helper-protocol/update-system-and-install.json");

const INSTALL_SUMMARY: &str = "install extra/dbeaver with extra/jre-openjdk (5 packages)";
const UPDATE_SUMMARY: &str = "update system (3 packages)";
const BOTH_SUMMARY: &str = "update system and install extra/vlc (2 packages)";

const NOT_IMPLEMENTED: &str = "{\"finished\":{\"error\":\"not_implemented\"}}\n";

/// The protocol's request cap (2 MiB, `MAX_REQUEST_BYTES` in `crates/helper-protocol`).
const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;

fn assert_inside_the_test_container() {
    let guard = require_container();
    assert!(
        guard.is_ok(),
        "refusing to run: {}",
        guard.err().map(|why| why.to_string()).unwrap_or_default()
    );
}

// ---- a fake journald ------------------------------------------------------------------------------------

struct FakeJournal {
    socket: UnixDatagram,
}

impl FakeJournal {
    fn start() -> FakeJournal {
        fs::create_dir_all(JOURNAL_DIR).unwrap();
        let _ = fs::remove_file(JOURNAL_SOCKET);
        let socket = UnixDatagram::bind(JOURNAL_SOCKET).unwrap();
        socket.set_nonblocking(true).unwrap();
        FakeJournal { socket }
    }

    /// Everything that was sent so far (the helper has already exited, so the datagrams are queued).
    fn datagrams(&self) -> Vec<String> {
        let mut found = Vec::new();
        let mut buffer = vec![0u8; 64 * 1024];
        while let Ok(n) = self.socket.recv(&mut buffer) {
            found.push(String::from_utf8_lossy(&buffer[..n]).into_owned());
        }
        found
    }
}

impl Drop for FakeJournal {
    fn drop(&mut self) {
        let _ = fs::remove_file(JOURNAL_SOCKET);
        let _ = fs::remove_dir(JOURNAL_DIR);
        let _ = fs::remove_dir("/run/systemd");
    }
}

// ---- running the helper ---------------------------------------------------------------------------------

struct Ran {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

/// Starts the helper with an empty environment plus `envs`, stdin as a pipe.
fn spawn(
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
    stdout: Stdio,
    stderr: Stdio,
) -> Child {
    Command::new(program)
        .args(args)
        .env_clear()
        .envs(envs.iter().copied())
        .stdin(Stdio::piped())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .unwrap()
}

/// Writes `stdin` to the child on a thread (so a child that stops reading cannot block us) and closes it.
fn feed(child: &mut Child, stdin: &[u8]) -> std::thread::JoinHandle<()> {
    let mut pipe = child.stdin.take().unwrap();
    let data = stdin.to_vec();
    std::thread::spawn(move || {
        let _ = pipe.write_all(&data);
    })
}

fn run_with(args: &[&str], envs: &[(&str, &str)], stdin: &[u8]) -> Ran {
    let mut child = spawn(HELPER, args, envs, Stdio::piped(), Stdio::piped());
    let writer = feed(&mut child, stdin);
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    Ran {
        status: output.status,
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

fn run(summary: &str, stdin: &[u8]) -> Ran {
    run_with(&[summary], &[], stdin)
}

fn full() -> Stdio {
    Stdio::from(OpenOptions::new().write(true).open("/dev/full").unwrap())
}

fn wait_up_to(child: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn pacman_installed() -> String {
    let output = Command::new("/usr/bin/pacman")
        .arg("-Q")
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// (size, mode, mtime seconds, mtime nanoseconds) of everything under the folders where a helper could do harm.
/// Metadata only: no file is opened, no link is followed.
fn snapshot() -> BTreeMap<String, (u64, u32, i64, i64)> {
    fn walk(path: &Path, into: &mut BTreeMap<String, (u64, u32, i64, i64)>) {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return;
        };
        into.insert(
            path.display().to_string(),
            (meta.size(), meta.mode(), meta.mtime(), meta.mtime_nsec()),
        );
        if meta.is_dir()
            && let Ok(entries) = fs::read_dir(path)
        {
            for entry in entries.flatten() {
                walk(&entry.path(), into);
            }
        }
    }
    let mut files = BTreeMap::new();
    for root in [
        "/etc",
        "/usr",
        "/var/lib/pacman",
        "/var/cache/pacman",
        "/var/log",
    ] {
        walk(&PathBuf::from(root), &mut files);
    }
    files
}

fn assert_fields(datagram: &str, expected: &[(&str, &str)]) {
    for (key, value) in expected {
        let line = format!("{key}={value}\n");
        assert!(datagram.contains(&line), "missing {line:?} in {datagram:?}");
    }
}

// ---- the good path --------------------------------------------------------------------------------------

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn each_sample_request_with_its_own_summary_ends_in_not_implemented() {
    assert_inside_the_test_container();
    for (request, summary) in [
        (INSTALL, INSTALL_SUMMARY),
        (UPDATE, UPDATE_SUMMARY),
        (BOTH, BOTH_SUMMARY),
    ] {
        let journal = FakeJournal::start();
        let ran = run_with(&[summary], &[("PKEXEC_UID", "1000")], request.as_bytes());
        assert_eq!(ran.status.code(), Some(1), "{summary}: {}", ran.stderr);
        assert_eq!(ran.stdout, NOT_IMPLEMENTED, "{summary}");
        assert_eq!(ran.stderr, "", "{summary}");

        let entries = journal.datagrams();
        assert_eq!(entries.len(), 1, "{summary}: {entries:?}");
        assert_fields(
            &entries[0],
            &[
                ("SYSLOG_IDENTIFIER", "tidewise-helper"),
                ("PRIORITY", "6"),
                ("TIDEWISE_CALLER_UID", "1000"),
                ("TIDEWISE_OUTCOME", "not_implemented"),
                ("TIDEWISE_SUMMARY", summary),
            ],
        );
    }
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn a_wrong_summary_is_refused_and_the_argv_text_is_not_logged() {
    assert_inside_the_test_container();
    let journal = FakeJournal::start();
    let ran = run("SECRET-ARGV-TEXT", UPDATE.as_bytes());
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(
        ran.stdout,
        "{\"finished\":{\"error\":\"summary_mismatch\"}}\n"
    );

    let entries = journal.datagrams();
    assert_eq!(entries.len(), 1);
    assert_fields(
        &entries[0],
        &[
            ("PRIORITY", "4"),
            ("TIDEWISE_OUTCOME", "summary_mismatch"),
            ("TIDEWISE_SUMMARY", UPDATE_SUMMARY),
        ],
    );
    assert!(!entries[0].contains("SECRET"), "{}", entries[0]);
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn hostile_requests_get_their_codes_and_nothing_from_them_reaches_the_journal() {
    assert_inside_the_test_container();
    let v2 = UPDATE.replace("\"protocol_version\":1", "\"protocol_version\":2");
    let force = UPDATE.replace("\"name\":\"glibc\"", "\"name\":\"--force\"");
    let secret = UPDATE.replace("\"name\":\"glibc\"", "\"name\":\"SECRET-PATH/etc/shadow\"");
    let over_the_cap = vec![b' '; MAX_REQUEST_BYTES + 1];
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        ("empty", Vec::new(), "malformed"),
        ("broken json", b"{".to_vec(), "malformed"),
        ("not utf-8", vec![0xff, 0xfe, 0x00], "malformed"),
        (
            "trailing data",
            format!("{UPDATE}x").into_bytes(),
            "malformed",
        ),
        ("over the cap", over_the_cap, "too_large"),
        ("version 2", v2.into_bytes(), "version_mismatch"),
        ("--force as a name", force.into_bytes(), "invalid"),
        ("a path as a name", secret.into_bytes(), "invalid"),
    ];
    for (what, stdin, code) in cases {
        let journal = FakeJournal::start();
        let ran = run(UPDATE_SUMMARY, &stdin);
        assert_eq!(ran.status.code(), Some(1), "{what}");
        assert!(
            ran.stdout
                .starts_with(&format!("{{\"finished\":{{\"error\":\"{code}\"}}}}\n"))
                || (code == "invalid"
                    && ran
                        .stdout
                        .starts_with("{\"finished\":{\"error\":{\"invalid\":")),
            "{what}: {}",
            ran.stdout
        );
        assert_eq!(ran.stdout.matches('\n').count(), 1, "{what}: one line only");
        assert!(
            !ran.stdout.contains("SECRET") && !ran.stderr.contains("SECRET"),
            "{what}"
        );

        let entries = journal.datagrams();
        assert_eq!(entries.len(), 1, "{what}");
        assert_fields(
            &entries[0],
            &[("PRIORITY", "4"), ("TIDEWISE_OUTCOME", code)],
        );
        assert!(
            !entries[0].contains("TIDEWISE_SUMMARY"),
            "{what}: {}",
            entries[0]
        );
        assert!(!entries[0].contains("SECRET"), "{what}: {}", entries[0]);
    }
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn without_a_journal_the_outcome_and_the_exit_code_are_the_same() {
    assert_inside_the_test_container();
    assert!(
        !Path::new(JOURNAL_SOCKET).exists(),
        "this test needs no journal"
    );
    let ran = run(UPDATE_SUMMARY, UPDATE.as_bytes());
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(ran.stdout, NOT_IMPLEMENTED);
    assert_eq!(ran.stderr, "");
}

// ---- how it is started ----------------------------------------------------------------------------------

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn the_wrong_number_of_arguments_is_refused_to_start_with_exit_2() {
    assert_inside_the_test_container();
    for args in [&[][..], &["a", "b"][..], &["--help", UPDATE_SUMMARY][..]] {
        let journal = FakeJournal::start();
        let ran = run_with(args, &[], UPDATE.as_bytes());
        assert_eq!(ran.status.code(), Some(2), "{args:?}");
        assert_eq!(ran.stdout, "", "{args:?}");
        assert!(
            ran.stderr
                .starts_with("tidewise-helper: it must be started with exactly one argument")
                && ran.stderr.ends_with("Nothing was changed.\n"),
            "{args:?}: {}",
            ran.stderr
        );
        assert!(
            journal.datagrams().is_empty(),
            "{args:?}: a refusal to start is not logged"
        );
    }
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn an_inherited_file_descriptor_is_refused_to_start() {
    assert_inside_the_test_container();
    // flock(1) opens the lock file and runs the command with that descriptor still open (the lock lives as long as
    // the command), so the helper starts with a descriptor 3 that is not its own. No shell is involved.
    let journal = FakeJournal::start();
    let mut child = Command::new("/usr/sbin/flock")
        .args(["/tmp/tidewise-fd-test.lock", HELPER, UPDATE_SUMMARY])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let writer = feed(&mut child, UPDATE.as_bytes());
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    let _ = fs::remove_file("/tmp/tidewise-fd-test.lock");

    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unexpected open files"), "{stderr}");
    assert!(journal.datagrams().is_empty());
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn a_hostile_environment_is_ignored_and_the_caller_is_logged_as_unknown() {
    assert_inside_the_test_container();
    let hostile = [
        ("PKEXEC_UID", "0\nMESSAGE=pwned"),
        ("PATH", "/tmp/evil"),
        ("IFS", "x"),
        ("PACMAN_ROOT", "/tmp/evil"),
        ("LC_ALL", "tr_TR.UTF-8"),
        ("HOME", "/tmp/evil"),
    ];
    let journal = FakeJournal::start();
    let ran = run_with(&[UPDATE_SUMMARY], &hostile, UPDATE.as_bytes());
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(ran.stdout, NOT_IMPLEMENTED);
    assert_eq!(ran.stderr, "");

    let entries = journal.datagrams();
    assert_eq!(entries.len(), 1);
    assert_fields(&entries[0], &[("TIDEWISE_CALLER_UID", "unknown")]);
    assert!(!entries[0].contains("pwned"), "{}", entries[0]);
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn it_does_not_read_stdin_when_it_refuses_to_start() {
    assert_inside_the_test_container();
    // stdin is a pipe that is never written to and never closed: a helper that read it would hang.
    let mut child = spawn(HELPER, &[], &[], Stdio::null(), Stdio::null());
    let status = wait_up_to(&mut child, Duration::from_secs(10));
    assert_eq!(
        status.and_then(|s| s.code()),
        Some(2),
        "it did not exit on its own"
    );
}

// ---- stdout or stderr that cannot be written (human's addition) ----------------------------------------

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn a_full_stdout_does_not_panic_the_journal_entry_is_written_and_the_exit_code_is_1() {
    assert_inside_the_test_container();
    let journal = FakeJournal::start();
    let mut child = spawn(HELPER, &[UPDATE_SUMMARY], &[], full(), Stdio::piped());
    let writer = feed(&mut child, UPDATE.as_bytes());
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    // A panic would be exit code 101.
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entries = journal.datagrams();
    assert_eq!(entries.len(), 1);
    assert_fields(&entries[0], &[("TIDEWISE_OUTCOME", "not_implemented")]);
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn a_stdout_whose_reader_has_gone_does_not_panic_either() {
    assert_inside_the_test_container();
    let journal = FakeJournal::start();
    let mut child = spawn(
        HELPER,
        &[UPDATE_SUMMARY],
        &[],
        Stdio::piped(),
        Stdio::piped(),
    );
    // Close our end before the helper has read its request, so its write fails with a broken pipe.
    drop(child.stdout.take());
    let writer = feed(&mut child, UPDATE.as_bytes());
    let status = wait_up_to(&mut child, Duration::from_secs(10));
    writer.join().unwrap();
    assert_eq!(status.and_then(|s| s.code()), Some(1));
    assert_eq!(journal.datagrams().len(), 1);
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn a_full_or_gone_stderr_does_not_panic_the_refusal_to_start_is_still_exit_2() {
    assert_inside_the_test_container();
    let journal = FakeJournal::start();

    let mut full_stderr = spawn(HELPER, &[], &[], Stdio::piped(), full());
    drop(full_stderr.stdin.take());
    let status = wait_up_to(&mut full_stderr, Duration::from_secs(10));
    assert_eq!(status.and_then(|s| s.code()), Some(2));

    let mut gone_stderr = spawn(HELPER, &[], &[], Stdio::piped(), Stdio::piped());
    drop(gone_stderr.stderr.take());
    drop(gone_stderr.stdin.take());
    let status = wait_up_to(&mut gone_stderr, Duration::from_secs(10));
    assert_eq!(status.and_then(|s| s.code()), Some(2));

    assert!(journal.datagrams().is_empty());
}

// ---- nothing changed ------------------------------------------------------------------------------------

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn nothing_on_the_system_changes() {
    assert_inside_the_test_container();
    let packages_before = pacman_installed();
    let files_before = snapshot();
    assert!(
        files_before.len() > 1000,
        "the snapshot looks empty: {}",
        files_before.len()
    );

    let journal = FakeJournal::start();
    let force = UPDATE.replace("\"name\":\"glibc\"", "\"name\":\"--force\"");
    let over_the_cap = vec![b' '; MAX_REQUEST_BYTES + 1];
    // Valid, mismatching, hostile and refused-to-start, so every path through the gate has run as root.
    for (summary, stdin) in [
        (INSTALL_SUMMARY, INSTALL.as_bytes().to_vec()),
        (UPDATE_SUMMARY, UPDATE.as_bytes().to_vec()),
        (BOTH_SUMMARY, BOTH.as_bytes().to_vec()),
        ("not the summary", UPDATE.as_bytes().to_vec()),
        (UPDATE_SUMMARY, force.into_bytes()),
        (UPDATE_SUMMARY, over_the_cap),
        (UPDATE_SUMMARY, b"{".to_vec()),
        (UPDATE_SUMMARY, Vec::new()),
    ] {
        let ran = run(summary, &stdin);
        assert_eq!(ran.status.code(), Some(1));
    }
    assert_eq!(run_with(&[], &[], b"").status.code(), Some(2));
    assert_eq!(run_with(&["a", "b"], &[], b"").status.code(), Some(2));
    assert_eq!(
        journal.datagrams().len(),
        8,
        "one entry per request that got past the start checks"
    );
    drop(journal);

    assert_eq!(
        pacman_installed(),
        packages_before,
        "the installed packages changed"
    );
    let files_after = snapshot();
    let changed: std::collections::BTreeSet<&String> = files_before
        .keys()
        .chain(files_after.keys())
        .filter(|path| files_before.get(*path) != files_after.get(*path))
        .collect();
    assert!(
        changed.is_empty(),
        "these paths changed: {:?}",
        changed.iter().take(10).collect::<Vec<_>>()
    );
    assert!(
        !Path::new("/var/lib/pacman/db.lck").exists(),
        "the helper must never create the lock"
    );
}
