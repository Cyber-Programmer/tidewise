//! What real pacman does, as permanent tests (WBS 5.3b). The helper's safety rests on these facts, so they are
//! measured again on every container run instead of being believed: how a transaction answers its questions under
//! `--noconfirm` and `--ask 66`, what a dry run does and does not notice, what a held lock blocks, and what happens to
//! pacman when its output pipe or its process group is hit by a signal (the reasons the helper writes pacman's output
//! to files and starts it in its own process group).
//!
//! Real pacman, as root, inside the Tidewise test container only (`tests/container/run.sh`). `--ask 66` is
//! 2 (replace) + 64 (import a PGP key) from `alpm_question_type_t` in `/usr/include/alpm.h`. Flag 64 cannot be reached
//! offline (libalpm looks the key up on the network before it asks), so it is not tested here.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

mod common;
use common::*;

const LOCK: &str = "/var/lib/pacman/db.lck";

fn qualified(name: &str) -> String {
    format!("{REPO}/tidewise-dummy-{name}")
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_conflict_aborts_under_noconfirm_and_installs_nothing_but_the_dry_run_does_not_notice() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-conf-x"]);

    // The dry run says nothing about the conflict (it prints the target and exits 0).
    let dry = pacman(&[
        "-S",
        "--print",
        "--print-format",
        "%r/%n %v",
        "--",
        &qualified("conf-y"),
    ]);
    assert!(dry.status.success());
    assert_eq!(
        String::from_utf8_lossy(&dry.stdout),
        format!("{REPO}/tidewise-dummy-conf-y 1.0-1\n")
    );

    // The real run: "Remove conf-x? [y/N]" defaults to no, so it aborts.
    let real = pacman(&[
        "-S",
        "--noconfirm",
        "--ask",
        "66",
        "--",
        &qualified("conf-y"),
    ]);
    assert!(!real.status.success());
    let stderr = String::from_utf8_lossy(&real.stderr);
    assert!(
        stderr.contains("unresolvable package conflicts detected"),
        "{stderr}"
    );
    assert_eq!(installed_version("tidewise-dummy-conf-y"), None);
    assert_eq!(
        installed_version("tidewise-dummy-conf-x").as_deref(),
        Some("1.0-1")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_named_provider_is_never_asked_and_the_dry_run_never_asks() {
    assert_inside_the_test_container();
    reset_world();
    let names = [qualified("needs-virtual"), qualified("prov-b")];

    // Named: the dry run lists the chosen provider, dependencies first.
    let dry = pacman(&[
        "-S",
        "--print",
        "--print-format",
        "%r/%n %v",
        "--",
        &names[0],
        &names[1],
    ]);
    assert_eq!(
        String::from_utf8_lossy(&dry.stdout),
        format!("{REPO}/tidewise-dummy-prov-b 1.0-1\n{REPO}/tidewise-dummy-needs-virtual 1.0-1\n")
    );

    // Not named: the dry run silently takes the first provider (it never asks).
    let unnamed = pacman(&[
        "-S",
        "--print",
        "--print-format",
        "%r/%n %v",
        "--",
        &names[0],
    ]);
    assert!(
        String::from_utf8_lossy(&unnamed.stdout).contains("tidewise-dummy-prov-a"),
        "{}",
        String::from_utf8_lossy(&unnamed.stdout)
    );

    // The real run with the provider named has no question to answer.
    let real = pacman(&[
        "-S",
        "--noconfirm",
        "--ask",
        "66",
        "--",
        &names[0],
        &names[1],
    ]);
    assert!(real.status.success());
    assert!(!String::from_utf8_lossy(&real.stdout).contains("providers available"));
    assert_eq!(
        installed_version("tidewise-dummy-prov-b").as_deref(),
        Some("1.0-1")
    );
    assert_eq!(installed_version("tidewise-dummy-prov-a"), None);
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_replacement_is_declined_by_ask_66_and_happens_without_it() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-old"]);
    add_newer("tidewise-dummy-new-1.0-1-any.pkg.tar.zst");

    // The dry run lists the replacing package and says nothing about the removal.
    let dry = pacman_stdout(&["-Su", "--print", "--print-format", "%r/%n %v"]);
    assert_eq!(dry, format!("{REPO}/tidewise-dummy-new 1.0-1\n"));

    // With --ask 66 the "Replace?" question is answered no: nothing is removed, nothing is installed, exit 0.
    let declined = pacman(&["-Su", "--noconfirm", "--ask", "66"]);
    assert!(declined.status.success());
    assert_eq!(
        installed_version("tidewise-dummy-old").as_deref(),
        Some("1.0-1")
    );
    assert_eq!(installed_version("tidewise-dummy-new"), None);
    // ... and it stays pending: the dry run still lists it.
    assert_eq!(
        pacman_stdout(&["-Su", "--print", "--print-format", "%r/%n %v"]),
        format!("{REPO}/tidewise-dummy-new 1.0-1\n")
    );

    // Without it (the default answer is yes) pacman removes the old package. This is what the flag prevents.
    let accepted = pacman(&["-Su", "--noconfirm"]);
    assert!(accepted.status.success());
    assert_eq!(installed_version("tidewise-dummy-old"), None);
    assert_eq!(
        installed_version("tidewise-dummy-new").as_deref(),
        Some("1.0-1")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_explicitly_named_ignored_package_is_installed_and_flag_1_would_decline_it() {
    assert_inside_the_test_container();
    reset_world();
    let conf = fs::read_to_string("/etc/pacman.conf").unwrap();
    fs::write(
        "/etc/pacman.conf",
        conf.replacen(
            "[options]\n",
            "[options]\nIgnorePkg = tidewise-dummy-hello\n",
            1,
        ),
    )
    .unwrap();

    // 66 does not include flag 1: the target is in the confirmed list, so it is installed.
    let real = pacman(&[
        "-S",
        "--noconfirm",
        "--ask",
        "66",
        "--",
        &qualified("hello"),
    ]);
    assert!(real.status.success());
    assert_eq!(
        installed_version("tidewise-dummy-hello").as_deref(),
        Some("1.0-1")
    );

    // Flag 1 inverts that question (shown only to prove the flags do what the helper's choice of 66 assumes).
    let _ = pacman(&["-Rdd", "--noconfirm", "--", "tidewise-dummy-hello"]);
    let declined = pacman(&["-S", "--noconfirm", "--ask", "1", "--", &qualified("hello")]);
    assert!(declined.status.success());
    assert_eq!(installed_version("tidewise-dummy-hello"), None);
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_held_lock_does_not_stop_dry_runs_and_stops_every_real_run() {
    assert_inside_the_test_container();
    reset_world();
    fs::write(LOCK, b"").unwrap();
    let before = fs::metadata(LOCK).unwrap();

    // Reads and dry runs go on (so the helper must look at the lock file itself).
    assert!(pacman(&["-Qu"]).stdout.is_empty());
    let dry = pacman(&[
        "-S",
        "--print",
        "--print-format",
        "%r/%n %v",
        "--",
        &qualified("hello"),
    ]);
    assert!(dry.status.success());
    assert!(
        pacman(&["-Su", "--print", "--print-format", "%r/%n %v"])
            .status
            .success()
    );

    // Everything that would write fails with pacman's lock messages, and pacman's advice is to remove the lock.
    for args in [
        vec!["-Sy"],
        vec![
            "-S",
            "--noconfirm",
            "--ask",
            "66",
            "--",
            "tidewise-dummy-hello",
        ],
        vec!["-D", "--asdeps", "--", "tidewise-dummy-hello"],
    ] {
        let output = pacman(&args);
        assert!(!output.status.success(), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("unable to lock database"),
            "{args:?}: {stderr}"
        );
        assert!(
            stderr.contains("could not lock database") || stderr.contains("synchronize"),
            "{stderr}"
        );
    }
    let after = fs::metadata(LOCK).unwrap();
    assert_eq!(
        (before.ino(), before.mtime(), before.mtime_nsec()),
        (after.ino(), after.mtime(), after.mtime_nsec())
    );
    let _ = fs::remove_file(LOCK); // the test container's own hygiene: the helper never does this
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn asdeps_marks_only_what_it_is_given() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-needs-virtual", "tidewise-dummy-prov-b"]);
    assert_eq!(
        install_reason("tidewise-dummy-prov-b"),
        "Explicitly installed"
    );
    let marked = pacman(&["-D", "--asdeps", "--", "tidewise-dummy-prov-b"]);
    assert!(marked.status.success());
    assert_eq!(
        install_reason("tidewise-dummy-prov-b"),
        "Installed as a dependency for another package"
    );
    assert_eq!(
        install_reason("tidewise-dummy-needs-virtual"),
        "Explicitly installed"
    );

    let missing = pacman(&["-D", "--asdeps", "--", "tidewise-dummy-not-there"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("could not set install reason"));
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn off_a_terminal_pacman_prints_the_phase_lines_and_package_lines_the_progress_tracker_reads() {
    assert_inside_the_test_container();
    reset_world();
    let real = pacman(&[
        "-S",
        "--noconfirm",
        "--ask",
        "66",
        "--",
        &qualified("scriptlet"),
    ]);
    assert!(real.status.success());
    let text = String::from_utf8_lossy(&real.stdout);
    let lines: Vec<&str> = text.lines().collect();
    for expected in [
        "resolving dependencies...",
        ":: Retrieving packages...",
        ":: Processing package changes...",
        "installing tidewise-dummy-scriptlet...",
        ":: Running post-transaction hooks...",
    ] {
        assert!(
            lines.contains(&expected),
            "missing {expected:?} in {lines:?}"
        );
    }
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with(' ') && l.ends_with(" downloading...")),
        "{lines:?}"
    );
    // No "(1/1)" counter on the package lines off a terminal.
    assert!(
        !lines.iter().any(|l| l.starts_with("(1/1) installing")),
        "{lines:?}"
    );
}

// ---- why the helper does not use pipes, and why pacman gets its own process group ------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_reader_that_goes_away_kills_pacman_with_sigpipe_and_leaves_the_lock() {
    assert_inside_the_test_container();
    reset_world();
    let mut child = Command::new("/usr/bin/pacman")
        .args([
            "-S",
            "--noconfirm",
            "--ask",
            "66",
            "--",
            &qualified("slow-a"),
        ])
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // The reader goes away before pacman has written much.
    drop(child.stdout.take());
    let status = wait_up_to(&mut child, Duration::from_secs(30)).expect("pacman did not stop");
    assert_eq!(
        status.signal(),
        Some(13),
        "killed by SIGPIPE, got {status:?}"
    );
    assert!(Path::new(LOCK).exists(), "the stale lock is left behind");
    assert_eq!(installed_version("tidewise-dummy-slow-a"), None);
    let _ = fs::remove_file(LOCK);
}

/// Starts a real two-package slow install with output to /dev/null and returns the child once the first scriptlet runs.
fn start_slow_install() -> std::process::Child {
    let child = Command::new("/usr/bin/pacman")
        .args([
            "-S",
            "--noconfirm",
            "--ask",
            "66",
            "--",
            &qualified("slow-a"),
            &qualified("slow-b"),
        ])
        .env_clear()
        .env("PATH", "/usr/bin")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = wait_for_file("/tmp/tidewise-slow-a-started", Duration::from_secs(20))
        || wait_for_file("/tmp/tidewise-slow-b-started", Duration::from_secs(1));
    assert!(started, "the first slow scriptlet never started");
    child
}

fn installed_slow_packages() -> usize {
    ["tidewise-dummy-slow-a", "tidewise-dummy-slow-b"]
        .iter()
        .filter(|name| installed_version(name).is_some())
        .count()
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn sighup_in_the_middle_of_a_commit_stops_pacman_after_one_package_and_still_exits_0() {
    assert_inside_the_test_container();
    reset_world();
    let mut child = start_slow_install();
    let pid = child.id().to_string();
    let signalled = Command::new("/usr/bin/kill")
        .args(["-HUP", &pid])
        .status()
        .unwrap();
    assert!(signalled.success());
    let status = wait_up_to(&mut child, Duration::from_secs(30)).expect("pacman did not stop");
    // A half transaction that looks like success: this is why a terminal's SIGHUP must never reach pacman.
    assert_eq!(status.code(), Some(0), "{status:?}");
    assert_eq!(
        installed_slow_packages(),
        1,
        "exactly one of the two packages was installed"
    );
    assert!(!Path::new(LOCK).exists());
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn sigterm_in_the_middle_of_a_commit_kills_pacman_and_leaves_the_lock() {
    assert_inside_the_test_container();
    reset_world();
    let mut child = start_slow_install();
    let pid = child.id().to_string();
    let signalled = Command::new("/usr/bin/kill")
        .args(["-TERM", &pid])
        .status()
        .unwrap();
    assert!(signalled.success());
    let status = wait_up_to(&mut child, Duration::from_secs(30)).expect("pacman did not stop");
    assert_eq!(status.signal(), Some(15), "{status:?}");
    assert!(
        Path::new(LOCK).exists(),
        "the stale lock is left behind (the helper never removes it)"
    );
    let _ = fs::remove_file(LOCK);
}
