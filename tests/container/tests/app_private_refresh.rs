//! The app's own private list refresh (WBS 6.11, decision 0008): `tidewise_pacman::refresh_private_lists` itself,
//! run as `nobody` against the real pacman, fakeroot and setpriv. `app_private_lists.rs` holds the measurements it
//! rests on.
//!
//! As in `resolver.rs`, the test binary starts itself again as `nobody` ("child mode"); the parent, as root, sets the
//! world up. Before the child runs, the `DisableSandbox*` line the Docker image adds to pacman.conf is taken out, so
//! the child has the sandbox settings of a normal machine. `run.sh` starts this file without `no-new-privileges`.
//!
//! Real pacman, inside the Tidewise test container only (`tests/container/run.sh`).

use std::fs;
use std::os::unix::fs::{PermissionsExt, chown};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use tidewise_pacman::{Lists, RefreshWhy, pending_updates_in, refresh_private_lists};

mod common;
use common::*;

const NOBODY: u32 = 65534;
const RUNTIME: &str = "/tmp/tw-run";
const LISTS: &str = "/tmp/tw-run/tidewise/lists/";
const LATER_PACKAGE: &str = "tidewise-dummy-upd-2.0-1-any.pkg.tar.zst";
const IMAGE_CONF: &str = "/tmp/tw-image-pacman.conf";
const UPDATE: &str = "tidewise-dummy-upd 1.0-1 -> 2.0-1";

// ---- child mode: what runs as nobody -----------------------------------------------------------------------

/// Not a test of its own: without `TW_REFRESH` it does nothing and passes.
#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_child() {
    let Ok(why) = std::env::var("TW_REFRESH") else {
        return;
    };
    let why = match why.as_str() {
        "refusal" => RefreshWhy::AfterRefusal,
        _ => RefreshWhy::Opening,
    };
    let lists = refresh_private_lists(why);
    let dbpath = match &lists {
        Lists::Private(folder) => Some(folder.as_path()),
        Lists::Real(_) => None,
    };
    let updates: Vec<String> = pending_updates_in(dbpath)
        .unwrap()
        .iter()
        .map(|u| format!("{} {} -> {}", u.name, u.installed_version, u.new_version))
        .collect();
    println!("REFRESHED {lists:?} updates=[{}]", updates.join(","));
}

// ---- parent mode -------------------------------------------------------------------------------------------

fn child(why: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--ignored", "--exact", "app_private_child", "--nocapture"])
        .args(["--test-threads=1"])
        .env_clear()
        .env("XDG_RUNTIME_DIR", RUNTIME)
        .env("TW_REFRESH", why)
        .uid(NOBODY)
        .gid(NOBODY)
        .current_dir("/")
        .stdin(Stdio::null());
    command
}

fn refreshed(why: &str) -> String {
    let output = child(why).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find_map(|line| line.split_once("REFRESHED ").map(|(_, rest)| rest))
        .unwrap_or_else(|| {
            panic!(
                "the child printed no result: {stdout}{}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
    eprintln!("{why}: {line}");
    line.to_string()
}

fn private(updates: &str) -> String {
    format!("Private(\"{LISTS}\") updates=[{updates}]")
}

/// One installed package with a newer version only a refresh can know about.
fn start() {
    assert_inside_the_test_container();
    // Root's own set-up downloads need the image's setting back (root here has no CAP_SYS_ADMIN).
    if Path::new(IMAGE_CONF).exists() {
        fs::copy(IMAGE_CONF, "/etc/pacman.conf").unwrap();
    } else {
        fs::copy("/etc/pacman.conf", IMAGE_CONF).unwrap();
    }
    reset_world();
    install_directly(&["tidewise-dummy-upd"]);
    change_repository_without_syncing(LATER_PACKAGE);
    let kept: String = fs::read_to_string("/etc/pacman.conf")
        .unwrap()
        .lines()
        .filter(|line| !line.trim_start().starts_with("DisableSandbox"))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write("/etc/pacman.conf", kept).unwrap();

    let _ = fs::remove_dir_all(RUNTIME);
    fs::create_dir(RUNTIME).unwrap();
    fs::set_permissions(RUNTIME, fs::Permissions::from_mode(0o700)).unwrap();
    chown(RUNTIME, Some(NOBODY), Some(NOBODY)).unwrap();
}

fn real_state() -> (Vec<String>, String, String, bool) {
    (
        fingerprint(REAL_SYNC),
        pacman_log(),
        package_list(),
        Path::new("/var/lib/pacman/db.lck").exists(),
    )
}

/// The repository goes back to having nothing newer. Its time is set ahead: `pacman -Sy` downloads only a list that
/// is newer than the one it has, to the second, and these tests change it within one.
fn repository_without_the_update() {
    let db = format!("{REPO_DIR}/tidewise-test.db.tar.gz");
    fs::copy("/tmp/original-test-repo.db", &db).unwrap();
    fs::set_permissions(&db, fs::Permissions::from_mode(0o644)).unwrap();
    fs::File::options()
        .write(true)
        .open(&db)
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_a_refresh_finds_the_update_and_leaves_the_real_lists_alone() {
    start();
    let before = real_state();
    assert_eq!(refreshed("opening"), private(UPDATE));
    assert_eq!(real_state(), before, "the real state changed");
    assert!(!Path::new(&format!("{LISTS}db.lck")).exists());
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_opening_again_does_not_download_and_after_a_refusal_it_does() {
    start();
    assert_eq!(refreshed("opening"), private(UPDATE));
    repository_without_the_update();
    // Ten minutes have not passed: the copy is read as it is.
    assert_eq!(refreshed("opening"), private(UPDATE));
    assert_eq!(refreshed("refusal"), private(""));
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_a_left_over_private_lock_is_removed_and_the_refresh_works() {
    start();
    assert_eq!(refreshed("opening"), private(UPDATE));
    repository_without_the_update();
    let stale = format!("{LISTS}db.lck");
    fs::write(&stale, b"").unwrap();
    chown(&stale, Some(NOBODY), Some(NOBODY)).unwrap();
    assert_eq!(refreshed("refusal"), private(""));
    assert!(!Path::new(&stale).exists());
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_without_fakeroot_the_real_lists_are_used_and_nothing_is_made() {
    start();
    fs::rename("/usr/bin/fakeroot", "/usr/bin/fakeroot.away").unwrap();
    let got = refreshed("opening");
    fs::rename("/usr/bin/fakeroot.away", "/usr/bin/fakeroot").unwrap();
    assert_eq!(got, "Real(Unavailable) updates=[]");
    assert!(!Path::new(&format!("{RUNTIME}/tidewise")).exists());
}

// ---- a crash in the middle of a refresh ---------------------------------------------------------------------

/// The pids of the processes with this name that are not zombies.
fn pids(name: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in fs::read_dir("/proc").unwrap().flatten() {
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        if status.lines().next() == Some(&format!("Name:\t{name}")) && !status.contains("State:\tZ")
        {
            found.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    found
}

fn kill(name: &str) {
    for pid in pids(name) {
        let _ = Command::new("/usr/bin/kill").args(["-KILL", &pid]).status();
    }
    std::thread::sleep(Duration::from_millis(500));
}

/// Starts an "app" whose refresh cannot finish (the repository's database is a FIFO nobody writes to).
fn hanging_app() -> Child {
    let db = format!("{REPO_DIR}/tidewise-test.db.tar.gz");
    fs::remove_file(&db).unwrap();
    assert!(
        Command::new("/usr/bin/mkfifo")
            .args(["-m", "0644", &db])
            .status()
            .unwrap()
            .success()
    );
    let app = child("opening")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !Path::new(&format!("{LISTS}db.lck")).exists()
        && start.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(Path::new(&format!("{LISTS}db.lck")).exists());
    app
}

/// The reason the lock is handed to the child: after the app dies, its pacman goes on, and the next window must not
/// take that pacman's `db.lck` for a left-over one.
#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_after_a_crash_the_next_window_waits_60_s_and_leaves_the_running_pacman_alone() {
    start();
    let mut app = hanging_app();
    app.kill().unwrap();
    app.wait().unwrap();
    kill("fakeroot");
    assert!(
        !pids("pacman").is_empty(),
        "no orphaned pacman to test with"
    );

    let start = Instant::now();
    assert_eq!(refreshed("refusal"), "Real(Busy) updates=[]");
    let waited = start.elapsed();
    assert!(
        waited >= Duration::from_secs(60) && waited < Duration::from_secs(70),
        "{waited:?}"
    );
    assert!(Path::new(&format!("{LISTS}db.lck")).exists());
    assert!(!pids("pacman").is_empty());

    // Once that pacman is gone, its lock is a left-over one.
    kill("pacman");
    kill("faked");
    fs::remove_file(format!("{REPO_DIR}/tidewise-test.db.tar.gz")).unwrap();
    repository_without_the_update();
    assert_eq!(refreshed("refusal"), private(""));
}
