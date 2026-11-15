//! The gate of the real `tidewise-helper`, as real root inside the Tidewise test container (WBS 5.3a, kept in 5.3b).
//!
//! These tests are about everything that happens before a job runs: how the helper is started, hostile requests, the
//! summary check, hostile environments, inherited descriptors, a stdout or stderr that cannot be written, and that a
//! **refusal changes nothing on the system** and never touches the work folder or pacman. Jobs are in
//! `helper_jobs.rs`.
//!
//! They are `#[ignore]` and start with the container guard: run them with `tests/container/run.sh`. A fake journald (a
//! datagram socket bound to `/run/systemd/journal/socket`) receives what the helper logs.
//!
//! Not tested here, on purpose: `LD_PRELOAD` / `LD_LIBRARY_PATH`. The dynamic loader acts before `main`, so nothing
//! the helper does can defend against them; through `pkexec` they do not arrive (`pkexec(1)`: "a minimal known and
//! safe environment"). pkexec and polkit are not in the container, so this is only checked on a real machine (5.4).

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

mod common;
use common::*;

const UPDATE: &str = include_str!("../../fixtures/helper-protocol/update-system.json");
const UPDATE_SUMMARY: &str = "update system (3 packages)";

// ---- the summary check and hostile requests -------------------------------------------------------------

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn a_wrong_summary_is_refused_and_the_argv_text_is_not_logged() {
    assert_inside_the_test_container();
    reset_world();
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
            ("TIDEWISE_STAGE", "finished"),
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
    reset_world();
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
            &[
                ("PRIORITY", "4"),
                ("TIDEWISE_STAGE", "finished"),
                ("TIDEWISE_OUTCOME", code),
            ],
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
fn a_refusal_never_creates_the_work_folder_or_starts_pacman() {
    assert_inside_the_test_container();
    reset_world();
    assert!(!Path::new(WORK_DIR).exists());
    let log_before = fs::read("/var/log/pacman.log").unwrap_or_default();
    let ran = run("not the summary", UPDATE.as_bytes());
    assert_eq!(ran.status.code(), Some(1));
    assert!(
        !Path::new(WORK_DIR).exists(),
        "the work folder must not exist after a refusal"
    );
    assert_eq!(
        fs::read("/var/log/pacman.log").unwrap_or_default(),
        log_before
    );
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn without_a_journal_a_refusal_ends_the_same_way_and_says_so() {
    assert_inside_the_test_container();
    reset_world();
    assert!(
        !Path::new(JOURNAL_SOCKET).exists(),
        "this test needs no journal"
    );
    let ran = run("not the summary", UPDATE.as_bytes());
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(
        ran.lines(),
        vec![
            "{\"notice\":\"audit_entry_not_written\"}",
            "{\"finished\":{\"error\":\"summary_mismatch\"}}"
        ]
    );
    assert_eq!(ran.stderr, "");
}

// ---- how it is started ----------------------------------------------------------------------------------

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn the_wrong_number_of_arguments_is_refused_to_start_with_exit_2() {
    assert_inside_the_test_container();
    reset_world();
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
    reset_world();
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
    reset_world();
    let hostile = [
        ("PKEXEC_UID", "0\nMESSAGE=pwned"),
        ("PATH", "/tmp/evil"),
        ("IFS", "x"),
        ("PACMAN_ROOT", "/tmp/evil"),
        ("LC_ALL", "tr_TR.UTF-8"),
        ("HOME", "/tmp/evil"),
    ];
    let journal = FakeJournal::start();
    // A refusal, so the entry shows what the helper made of the caller without a job running.
    let ran = run_with(&["not the summary"], &hostile, UPDATE.as_bytes());
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(
        ran.stdout,
        "{\"finished\":{\"error\":\"summary_mismatch\"}}\n"
    );
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
    reset_world();
    // stdin is a pipe that is never written to and never closed: a helper that read it would hang.
    let mut child = spawn(HELPER, &[], &[], Stdio::null(), Stdio::null());
    let status = wait_up_to(&mut child, Duration::from_secs(10));
    assert_eq!(
        status.and_then(|s| s.code()),
        Some(2),
        "it did not exit on its own"
    );
}

// ---- stdout or stderr that cannot be written ------------------------------------------------------------

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn a_full_stdout_on_a_refusal_does_not_panic_and_the_entry_is_written() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    let mut child = spawn(HELPER, &["not the summary"], &[], full(), Stdio::piped());
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
    assert_fields(&entries[0], &[("TIDEWISE_OUTCOME", "summary_mismatch")]);
}

#[test]
#[ignore = "runs the root helper: only inside the test container (tests/container/run.sh)"]
fn a_full_or_gone_stderr_does_not_panic_the_refusal_to_start_is_still_exit_2() {
    assert_inside_the_test_container();
    reset_world();
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
fn nothing_on_the_system_changes_when_every_request_is_refused() {
    assert_inside_the_test_container();
    reset_world();
    let packages_before = package_list();
    let files_before = snapshot();
    assert!(
        files_before.len() > 1000,
        "the snapshot looks empty: {}",
        files_before.len()
    );

    let journal = FakeJournal::start();
    let force = UPDATE.replace("\"name\":\"glibc\"", "\"name\":\"--force\"");
    let over_the_cap = vec![b' '; MAX_REQUEST_BYTES + 1];
    // Every way to be refused after the start checks: a wrong summary, hostile and broken requests.
    for (summary, stdin) in [
        ("not the summary", UPDATE.as_bytes().to_vec()),
        ("", UPDATE.as_bytes().to_vec()),
        (UPDATE_SUMMARY, force.into_bytes()),
        (UPDATE_SUMMARY, over_the_cap),
        (UPDATE_SUMMARY, b"{".to_vec()),
        (UPDATE_SUMMARY, Vec::new()),
    ] {
        let ran = run(summary, &stdin);
        assert_eq!(ran.status.code(), Some(1));
    }
    // And the ways to refuse to start.
    assert_eq!(run_with(&[], &[], b"").status.code(), Some(2));
    assert_eq!(run_with(&["a", "b"], &[], b"").status.code(), Some(2));
    assert_eq!(
        journal.datagrams().len(),
        6,
        "one entry per request that got past the start checks"
    );
    drop(journal);

    assert_eq!(
        package_list(),
        packages_before,
        "the installed packages changed"
    );
    let files_after = snapshot();
    let changed = differences(&files_before, &files_after);
    assert!(changed.is_empty(), "these paths changed: {changed:?}");
    assert!(
        !Path::new("/var/lib/pacman/db.lck").exists(),
        "the helper must never create the lock"
    );
    assert!(
        !Path::new(WORK_DIR).exists(),
        "a refusal must not create the work folder"
    );
}
