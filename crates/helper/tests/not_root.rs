//! The real `tidewise-helper` binary, started as an ordinary user: it must refuse (threat T-01) and must say so
//! without ever panicking, even when stdout or stderr cannot be written (human's addition to 5.3a).
//!
//! These tests need to be *not* root. They skip (with a message) when run as root, for example inside the container,
//! where `tests/container/tests/helper.rs` covers the root cases.

use std::fs::OpenOptions;
use std::io::Write;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use tidewise_helper::run::StartRefusal;
use tidewise_helper::uid::{is_root, read_status};

const HELPER: &str = env!("CARGO_BIN_EXE_tidewise-helper");
const REQUEST: &str = include_str!("../../../tests/fixtures/helper-protocol/update-system.json");
const SUMMARY: &str = "update system (3 packages)";

/// True (after saying so) when this test run is root, so the test should do nothing.
fn running_as_root() -> bool {
    let root = read_status().map(|text| is_root(&text)).unwrap_or(false);
    if root {
        eprintln!("skipped: these tests are for a non-root user");
    }
    root
}

#[expect(
    clippy::disallowed_methods,
    reason = "starts the helper binary under test, as an ordinary user, with an argument list and no shell"
)]
fn helper(args: &[&str]) -> Command {
    let mut command = Command::new(HELPER);
    command.args(args).env_clear();
    command
}

fn full() -> Stdio {
    // /dev/full accepts the open and fails every write with "no space left on device".
    let file = OpenOptions::new().write(true).open("/dev/full").unwrap();
    Stdio::from(file)
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

fn expected_sentence() -> String {
    format!("{}\n", StartRefusal::NotRoot.text())
}

#[test]
fn as_an_ordinary_user_it_refuses_with_one_sentence_on_stderr_and_exit_2() {
    if running_as_root() {
        return;
    }
    let mut child = helper(&[SUMMARY])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The helper must not read this; a broken pipe here is fine.
    let _ = child.stdin.take().unwrap().write_all(REQUEST.as_bytes());
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        expected_sentence()
    );
}

/// `flock(1)` opens its lock file and runs the command with that descriptor still open, so the helper starts with a
/// descriptor 3 it did not expect (the same trick as the container test; no shell). Skips if flock is not installed.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "starts flock to hand the helper an extra descriptor, and removes the test's own lock file in the temp folder"
)]
fn an_extra_open_file_still_gets_the_not_root_message() {
    if running_as_root() {
        return;
    }
    let Some(flock) = ["/usr/bin/flock", "/usr/sbin/flock"]
        .into_iter()
        .find(|path| std::path::Path::new(path).exists())
    else {
        eprintln!("skipped: flock is not installed");
        return;
    };
    let lock =
        std::env::temp_dir().join(format!("tidewise-helper-extra-fd-{}", std::process::id()));
    let output = Command::new(flock)
        .arg(&lock)
        .arg(HELPER)
        .arg(SUMMARY)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&lock);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    // Not "unexpected open files": being started by hand as a normal user is the useful thing to say.
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        expected_sentence()
    );
}

#[test]
fn the_not_root_check_comes_before_the_argument_check() {
    if running_as_root() {
        return;
    }
    for args in [&[][..], &["a", "b"][..], &["--help"][..]] {
        let output = helper(args).stdin(Stdio::null()).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(output.stdout, b"", "{args:?}");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            expected_sentence(),
            "{args:?}"
        );
    }
}

// Not tested here, on purpose: LD_PRELOAD and LD_LIBRARY_PATH. The dynamic loader acts before `main`, so no code of
// ours can defend against them. Through `pkexec` they do not arrive: pkexec(1) starts the program with "a minimal
// known and safe environment". (Threat model T-08.)
#[test]
fn a_hostile_environment_changes_nothing() {
    if running_as_root() {
        return;
    }
    let output = helper(&[SUMMARY])
        .env("PKEXEC_UID", "0\nMESSAGE=pwned")
        .env("PATH", "/tmp/evil")
        .env("IFS", "x")
        .env("PACMAN_ROOT", "/tmp/evil")
        .env("LC_ALL", "tr_TR.UTF-8")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        expected_sentence()
    );
}

#[test]
fn it_does_not_wait_for_stdin_when_it_refuses() {
    if running_as_root() {
        return;
    }
    // stdin is a pipe that is never written to and never closed: a helper that read it would hang here.
    let mut child = helper(&[SUMMARY])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let status = wait_up_to(&mut child, Duration::from_secs(10));
    assert_eq!(
        status.and_then(|s| s.code()),
        Some(2),
        "it did not exit on its own"
    );
}

#[test]
fn a_full_stderr_does_not_panic_the_refusal_is_still_exit_2() {
    if running_as_root() {
        return;
    }
    let status = helper(&[SUMMARY])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(full())
        .status()
        .unwrap();
    // A panic would be exit code 101.
    assert_eq!(status.code(), Some(2));
}

#[test]
fn a_full_stdout_and_a_full_stderr_together_do_not_panic_either() {
    if running_as_root() {
        return;
    }
    let status = helper(&[SUMMARY])
        .stdin(Stdio::null())
        .stdout(full())
        .stderr(full())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(2));
}

#[test]
fn a_stderr_whose_reader_has_gone_does_not_panic_either() {
    if running_as_root() {
        return;
    }
    let mut child = helper(&[SUMMARY])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Close our end of the pipe before the helper can write: its write fails with a broken pipe.
    drop(child.stderr.take());
    let status = wait_up_to(&mut child, Duration::from_secs(10));
    assert_eq!(status.and_then(|s| s.code()), Some(2));
}
