//! What real pacman does when an ordinary user refreshes a private copy of the package lists (WBS 6.11). These are
//! measurements, run before any app code depends on them. The parent (the container's root) sets the world up; every
//! pacman call that matters runs as `nobody`, because the app never runs as root.
//!
//! Everything is printed (`--nocapture` shows it) so decision 0008 can quote it.
//!
//! The image's own pacman.conf switches the filesystem sandbox off (the Arch Docker image adds the line). A normal
//! machine does not, so the user's refresh gets `--config` with a copy that has no `DisableSandbox*` line. The system
//! pacman.conf stays as it is, for root's set-up work. `--config` is only a device of this test.
//!
//! `run.sh` starts containers with `no-new-privileges`. A user's session does not have that bit, and it decides whether
//! pacman may apply its filesystem sandbox without privilege: with the bit set it can, without it the refresh fails.
//! So the test reads the bit and expects the matching result. **`run.sh` alone therefore never shows the failure**; to see
//! it, start the built binary with `podman run` without `--security-opt=no-new-privileges` (measured 25 Nov 2026).
//!
//! Real pacman, inside the Tidewise test container only (`tests/container/run.sh`).

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, chown, symlink};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

mod common;
use common::*;

const NOBODY: u32 = 65534;
const LATER_PACKAGE: &str = "tidewise-dummy-upd-2.0-1-any.pkg.tar.zst";
const BASE: &str = "/tmp/tw-app";
const LISTS: &str = "/tmp/tw-app/lists/";
const FAKEROOT: &str = "/usr/bin/fakeroot";
const SETPRIV: &str = "/usr/bin/setpriv";
const PACMAN: &str = "/usr/bin/pacman";
const USER_CONF: &str = "/tmp/tw-app/pacman.conf";
const NO_FS_SANDBOX: &[&str] = &["--disable-sandbox-filesystem"];

// ---- setting the world up (as root) -----------------------------------------------------------------------

fn start() {
    assert_inside_the_test_container();
    reset_world();
    let _ = fs::remove_dir_all(BASE);
    folder(BASE, 0o755);
    let kept: String = fs::read_to_string("/etc/pacman.conf")
        .unwrap()
        .lines()
        .filter(|line| !line.trim_start().starts_with("DisableSandbox"))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write(USER_CONF, kept).unwrap();
    fs::set_permissions(USER_CONF, fs::Permissions::from_mode(0o644)).unwrap();
}

/// The folder the app would make: owned by the user, 0700, with only the `local` link. pacman makes `sync/` itself.
fn user_folder() {
    let _ = fs::remove_dir_all(LISTS);
    folder(LISTS, 0o700);
    symlink("/var/lib/pacman/local", format!("{LISTS}local")).unwrap();
    chown(LISTS, Some(NOBODY), Some(NOBODY)).unwrap();
}

// ---- running as nobody ------------------------------------------------------------------------------------

fn command_as_nobody(program: &str, args: &[&str], envs: &[(&str, &str)]) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .envs(envs.iter().copied())
        .stdin(Stdio::null())
        .uid(NOBODY)
        .gid(NOBODY)
        .current_dir("/");
    command
}

fn as_nobody(program: &str, args: &[&str], envs: &[(&str, &str)]) -> Output {
    command_as_nobody(program, args, envs).output().unwrap()
}

/// The refresh of option A: `fakeroot -- pacman -Sy <extra> --dbpath <LISTS> --logfile /dev/null`.
fn refresh_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["--", PACMAN, "-Sy"];
    args.extend_from_slice(extra);
    args.extend(["--config", USER_CONF]);
    args.extend(["--dbpath", LISTS, "--logfile", "/dev/null"]);
    args
}

fn fakeroot_refresh(extra: &[&str]) -> Output {
    as_nobody(FAKEROOT, &refresh_args(extra), &[("PATH", "/usr/bin")])
}

fn show(label: &str, output: &Output) {
    eprintln!(
        "--- {label}: exit {:?}\n[stdout]\n{}[stderr]\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Every entry below `dir` as "path owner:group mode", not following links.
fn tree(dir: &str) -> Vec<String> {
    fn walk(path: &Path, into: &mut Vec<String>) {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return;
        };
        into.push(format!(
            "{} {}:{} {:o}{}",
            path.display(),
            meta.uid(),
            meta.gid(),
            meta.mode() & 0o7777,
            if meta.is_symlink() { " (link)" } else { "" }
        ));
        if meta.is_dir()
            && let Ok(entries) = fs::read_dir(path)
        {
            for entry in entries.flatten() {
                walk(&entry.path(), into);
            }
        }
    }
    let mut lines = Vec::new();
    walk(Path::new(dir), &mut lines);
    lines.sort();
    lines
}

/// Everything a refresh must not touch.
fn real_state() -> (Vec<String>, String, String, bool) {
    (
        fingerprint(REAL_SYNC),
        pacman_log(),
        pacman_stdout(&["-Q"]),
        Path::new("/var/lib/pacman/db.lck").exists(),
    )
}

/// Does `nobody` run with `no-new-privileges`? (A user's own session does not.)
fn no_new_privileges() -> bool {
    let status = as_nobody("/usr/bin/cat", &["/proc/self/status"], &[]);
    String::from_utf8_lossy(&status.stdout)
        .lines()
        .any(|line| line.starts_with("NoNewPrivs:") && line.ends_with('1'))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn private_list() -> String {
    format!("{LISTS}sync/tidewise-test.db")
}

// ---- the measurements -------------------------------------------------------------------------------------

/// The refresh the app starts: `setpriv --no-new-privs fakeroot -- pacman -Sy ...`. `setpriv` sets the bit a user's
/// session lacks and then replaces itself with `fakeroot`; the filesystem sandbox stays on.
fn app_refresh_args<'a>(extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["--no-new-privs", FAKEROOT];
    args.extend(refresh_args(extra));
    args
}

fn app_refresh(extra: &[&str]) -> Output {
    as_nobody(SETPRIV, &app_refresh_args(extra), &[])
}

/// The measurement the human asked for: from a process **without** `no-new-privileges`, the refresh through `setpriv`
/// works, and pacman says it applied the Landlock filesystem sandbox. The control is the same call without `setpriv`.
#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_13_setpriv_keeps_the_filesystem_sandbox_on() {
    start();
    eprintln!("the caller has no-new-privileges: {}", no_new_privileges());
    change_repository_without_syncing(LATER_PACKAGE);
    let before = real_state();

    user_folder();
    let run = app_refresh(&["--debug"]);
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    for line in all.lines() {
        let low = line.to_lowercase();
        if low.starts_with("error")
            || ["landlock", "seccomp", "sandbox"]
                .iter()
                .any(|word| low.contains(word))
        {
            eprintln!("{line}");
        }
    }
    assert!(run.status.success(), "{all}");
    assert!(
        all.contains("filesystem access has been restricted to /tmp/tw-app/lists/sync/download-"),
        "Landlock was not applied"
    );
    assert!(all.contains("Landlock ABI is"));
    assert!(all.contains("syscalls via seccomp"));
    assert!(Path::new(&private_list()).is_file());
    assert_eq!(real_state(), before, "the real state changed");

    if !no_new_privileges() {
        user_folder();
        let control = fakeroot_refresh(&[]);
        assert!(
            !control.status.success(),
            "the control worked without setpriv"
        );
        assert!(stderr(&control).contains("Landlock ruleset could not be applied"));
    }
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_0_versions_and_the_sandbox_settings() {
    start();
    eprintln!(
        "{}{}",
        pacman_stdout(&["-Q", "pacman", "fakeroot", "util-linux"]),
        fs::read_to_string(USER_CONF)
            .unwrap()
            .lines()
            .filter(|l| l.contains("DownloadUser") || l.contains("Sandbox"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let status = as_nobody("/usr/bin/cat", &["/proc/self/status"], &[]);
    let status = String::from_utf8_lossy(&status.stdout);
    for line in status.lines() {
        if line.starts_with("NoNewPrivs") || line.starts_with("Seccomp") {
            eprintln!("{line}");
        }
    }
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_1_plain_pacman_refuses_to_sync_as_a_normal_user() {
    start();
    user_folder();
    let before = tree(LISTS);
    let plain = as_nobody(
        PACMAN,
        &["-Sy", "--dbpath", LISTS, "--logfile", "/dev/null"],
        &[],
    );
    show("plain -Sy as nobody", &plain);
    assert!(!plain.status.success());
    assert!(stderr(&plain).contains("unless you are root"));
    assert_eq!(tree(LISTS), before, "something was written");
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_2_fakeroot_refresh_with_each_sandbox_flag() {
    start();
    change_repository_without_syncing(LATER_PACKAGE);
    let before = real_state();
    for extra in [
        &[][..],
        &["--disable-sandbox-filesystem"][..],
        &["--disable-sandbox-syscalls"][..],
        &["--disable-sandbox"][..],
    ] {
        user_folder();
        let run = fakeroot_refresh(extra);
        show(&format!("fakeroot -Sy {extra:?}"), &run);
        eprintln!("{}", tree(LISTS).join("\n"));
        eprintln!("list arrived: {}", Path::new(&private_list()).is_file());

        let filesystem_sandbox_off = extra
            .iter()
            .any(|flag| ["--disable-sandbox", "--disable-sandbox-filesystem"].contains(flag));
        let works = filesystem_sandbox_off || no_new_privileges();
        assert_eq!(run.status.success(), works, "{extra:?}");
        assert_eq!(Path::new(&private_list()).is_file(), works, "{extra:?}");
        if !works {
            assert!(stderr(&run).contains("Landlock ruleset could not be applied"));
        }
    }
    assert_eq!(real_state(), before, "the real state changed");
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_3_what_the_sandbox_does_under_fakeroot() {
    start();
    for extra in [&["--debug"][..], &["--debug", NO_FS_SANDBOX[0]][..]] {
        user_folder();
        let run = fakeroot_refresh(extra);
        eprintln!("{extra:?}: exit {:?}", run.status.code());
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        if extra.len() == 2 {
            assert!(run.status.success());
            assert!(
                all.contains("syscalls via seccomp"),
                "the syscall sandbox is off"
            );
        }
        for line in all.lines() {
            let low = line.to_lowercase();
            if low.starts_with("error")
                || ["sandbox", "landlock", "seccomp", "restrict"]
                    .iter()
                    .any(|word| low.contains(word))
            {
                eprintln!("{line}");
            }
        }
    }
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_4_whether_fakeroot_needs_path() {
    start();
    user_folder();
    let without = as_nobody(FAKEROOT, &refresh_args(NO_FS_SANDBOX), &[]);
    show("no PATH", &without);
    user_folder();
    let with = fakeroot_refresh(NO_FS_SANDBOX);
    show("PATH=/usr/bin", &with);
    assert!(without.status.success() && with.status.success());
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_5_reading_the_private_lists_as_the_user() {
    start();
    install_directly(&["tidewise-dummy-upd"]);
    change_repository_without_syncing(LATER_PACKAGE);
    user_folder();

    let before = real_state();
    show("refresh", &fakeroot_refresh(NO_FS_SANDBOX));
    let private = as_nobody(PACMAN, &["-Qu", "--dbpath", LISTS], &[]);
    show("-Qu private", &private);
    assert_eq!(
        String::from_utf8_lossy(&private.stdout),
        "tidewise-dummy-upd 1.0-1 -> 2.0-1\n"
    );
    let real = as_nobody(PACMAN, &["-Qu"], &[]);
    show("-Qu real", &real);
    assert_eq!((real.status.code(), real.stdout.len()), (Some(1), 0));
    show(
        "-Su --print private",
        &as_nobody(
            PACMAN,
            &[
                "-Su",
                "--print",
                "--print-format",
                "%r/%n %v %s",
                "--dbpath",
                LISTS,
            ],
            &[],
        ),
    );
    show(
        "second refresh, nothing new",
        &fakeroot_refresh(NO_FS_SANDBOX),
    );
    assert_eq!(real_state(), before, "the real state changed");
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_6_a_stale_private_lock() {
    start();
    user_folder();
    show("first refresh", &fakeroot_refresh(NO_FS_SANDBOX));
    change_repository_without_syncing(LATER_PACKAGE);
    fs::write(format!("{LISTS}db.lck"), b"").unwrap();
    chown(format!("{LISTS}db.lck"), Some(NOBODY), Some(NOBODY)).unwrap();
    let list_before = sha256(&private_list());

    let locked = fakeroot_refresh(NO_FS_SANDBOX);
    show("refresh with a stale lock", &locked);
    assert_eq!(locked.status.code(), Some(1));
    assert!(stderr(&locked).contains("unable to lock database"));
    eprintln!("{}", tree(LISTS).join("\n"));
    assert!(Path::new(&format!("{LISTS}db.lck")).exists());
    assert_eq!(sha256(&private_list()), list_before);
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_7_a_download_that_fails() {
    start();
    let db = format!("{REPO_DIR}/tidewise-test.db.tar.gz");
    let aside = "/tmp/tw-app/aside.db";

    // With no list yet.
    user_folder();
    fs::rename(&db, aside)
        .or_else(|_| fs::copy(&db, aside).and_then(|_| fs::remove_file(&db)))
        .unwrap();
    show(
        "failed refresh, no old list",
        &fakeroot_refresh(NO_FS_SANDBOX),
    );
    eprintln!("{}", tree(LISTS).join("\n"));
    fs::copy(aside, &db).unwrap();

    // With an old list.
    user_folder();
    show("good refresh", &fakeroot_refresh(NO_FS_SANDBOX));
    let old = sha256(&private_list());
    fs::remove_file(&db).unwrap();
    let failed = fakeroot_refresh(NO_FS_SANDBOX);
    show("failed refresh over an old list", &failed);
    assert_eq!(failed.status.code(), Some(1));
    assert!(stderr(&failed).contains("failed to retrieve some files"));
    eprintln!("{}", tree(LISTS).join("\n"));
    assert_eq!(sha256(&private_list()), old, "the old list was lost");
    assert!(!Path::new(&format!("{LISTS}db.lck")).exists());
    fs::copy(aside, &db).unwrap();
    fs::set_permissions(&db, fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_8_option_b_a_user_namespace() {
    start();
    for extra in [&[][..], &["--disable-sandbox"][..]] {
        user_folder();
        let mut args = vec!["--user", "--map-root-user"];
        args.extend(refresh_args(extra));
        let run = as_nobody("/usr/bin/unshare", &args, &[("PATH", "/usr/bin")]);
        show(&format!("unshare -Sy {extra:?}"), &run);
        eprintln!("list arrived: {}", Path::new(&private_list()).is_file());
        assert_eq!(run.status.success(), !extra.is_empty(), "{extra:?}");
    }
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_9_option_c_reading_the_helpers_copy() {
    start();
    install_directly(&["tidewise-dummy-upd"]);
    let query = ["-Qu", "--dbpath", PRIVATE_LISTS];
    let missing = as_nobody(PACMAN, &query, &[]);
    show("helper copy missing", &missing);
    assert!(!missing.status.success());

    change_repository_without_syncing(LATER_PACKAGE);
    private_folder(PRIVATE_LISTS, 0o711);
    let synced = pacman(&["-Sy", "--dbpath", PRIVATE_LISTS, "--logfile", "/dev/null"]);
    show("the helper's sync, as root", &synced);
    let present = as_nobody(PACMAN, &query, &[]);
    show("helper copy present", &present);
    assert!(String::from_utf8_lossy(&present.stdout).contains("1.0-1 -> 2.0-1"));
}

// ---- a refresh that is killed half-way --------------------------------------------------------------------

/// "pid ppid uid name" of every process called pacman, fakeroot or faked.
fn processes() -> Vec<String> {
    let mut found = Vec::new();
    for entry in fs::read_dir("/proc").unwrap().flatten() {
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let field = |name: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(name))
                .and_then(|v| v.split_whitespace().next())
                .unwrap_or("")
                .to_string()
        };
        let name = field("Name:");
        if ["pacman", "fakeroot", "faked"].contains(&name.as_str()) && field("State:") != "Z" {
            found.push(format!(
                "{} {} {} {name}",
                field("Pid:"),
                field("PPid:"),
                field("Uid:")
            ));
        }
    }
    found.sort();
    found
}

fn kill_all_found() {
    for line in processes() {
        let pid = line.split(' ').next().unwrap().to_string();
        let _ = Command::new("/usr/bin/kill").args(["-KILL", &pid]).status();
    }
    std::thread::sleep(Duration::from_millis(500));
}

/// Starts a refresh that cannot finish: the repository's database is a FIFO that nobody writes to.
fn hanging_refresh() -> Child {
    let db = format!("{REPO_DIR}/tidewise-test.db.tar.gz");
    fs::remove_file(&db).unwrap();
    assert!(
        Command::new("/usr/bin/mkfifo")
            .args(["-m", "0644", &db])
            .status()
            .unwrap()
            .success()
    );
    user_folder();
    let child = command_as_nobody(SETPRIV, &app_refresh_args(&[]), &[])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while !Path::new(&format!("{LISTS}db.lck")).exists() && start.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(500));
    child
}

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_10_what_survives_when_the_started_process_is_killed() {
    start();
    for signal in ["-TERM", "-KILL"] {
        let mut child = hanging_refresh();
        eprintln!(
            "=== {signal}: started pid {}, still running: {}",
            child.id(),
            child.try_wait().unwrap().is_none()
        );
        eprintln!("before (pid ppid uid name):\n{}", processes().join("\n"));
        let _ = Command::new("/usr/bin/kill")
            .args([signal, &child.id().to_string()])
            .status();
        std::thread::sleep(Duration::from_secs(1));
        eprintln!("started process ended: {:?}", child.try_wait().unwrap());
        eprintln!("survivors:\n{}", processes().join("\n"));
        assert!(
            processes().iter().any(|line| line.ends_with(" pacman")),
            "pacman did not outlive the process we started"
        );
        assert!(Path::new(&format!("{LISTS}db.lck")).exists());
        eprintln!(
            "private db.lck left: {}",
            Path::new(&format!("{LISTS}db.lck")).exists()
        );
        kill_all_found();
        let _ = child.wait();
        let _ = fs::remove_file(format!("{REPO_DIR}/tidewise-test.db.tar.gz"));
        fs::write(format!("{REPO_DIR}/tidewise-test.db.tar.gz"), b"").unwrap();
    }
}

// ---- two follow-ups the first run asked for ---------------------------------------------------------------

#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_11_option_b_with_each_flag_and_what_it_leaves() {
    start();
    change_repository_without_syncing(LATER_PACKAGE);
    let before = real_state();
    for extra in [
        &["--disable-sandbox-filesystem"][..],
        &["--disable-sandbox-syscalls"][..],
        &["--disable-sandbox", "--debug"][..],
    ] {
        user_folder();
        let mut args = vec!["--user", "--map-root-user"];
        args.extend(refresh_args(extra));
        let run = as_nobody("/usr/bin/unshare", &args, &[]);
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        eprintln!("--- unshare -Sy {extra:?}: exit {:?}", run.status.code());
        let whole_sandbox_off = extra.contains(&"--disable-sandbox");
        assert_eq!(run.status.success(), whole_sandbox_off, "{extra:?}");
        for line in all.lines() {
            let low = line.to_lowercase();
            if low.starts_with("error")
                || ["sandbox", "landlock", "seccomp"]
                    .iter()
                    .any(|w| low.contains(w))
            {
                eprintln!("{line}");
            }
        }
        eprintln!("{}", tree(LISTS).join("\n"));
    }
    assert_eq!(real_state(), before, "the real state changed");
}

/// Could a second app take the lock now? Asked on a fresh open of the file, as a second window would.
fn lock_is_free(path: &str) -> bool {
    fs::File::open(path).unwrap().try_lock().is_ok()
}

/// The app takes an `flock` on the private folder itself and hands the same open folder to the child (here as its stdin, which needs no `unsafe`),
/// so the lock lasts as long as anything the refresh started still has it. Who keeps it?
#[test]
#[ignore = "runs real pacman: only inside the test container (tests/container/run.sh)"]
fn app_private_12_a_lock_handed_to_the_child() {
    start();
    let lock_path = LISTS;
    let db = format!("{REPO_DIR}/tidewise-test.db.tar.gz");
    let handed = |lock: &fs::File| -> Command {
        let mut command = command_as_nobody(SETPRIV, &app_refresh_args(&[]), &[]);
        command
            .stdin(Stdio::from(lock.try_clone().unwrap()))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    };

    for signal in ["none", "-TERM", "-KILL"] {
        let original = fs::read("/tmp/original-test-repo.db").unwrap();
        let _ = fs::remove_file(&db);
        if signal == "none" {
            fs::write(&db, original).unwrap();
        } else {
            assert!(
                Command::new("/usr/bin/mkfifo")
                    .args(["-m", "0644", &db])
                    .status()
                    .unwrap()
                    .success()
            );
        }
        user_folder();
        let lock = fs::File::open(lock_path).unwrap();
        lock.lock().unwrap();
        // `Command` keeps its own copy of the descriptor until it is dropped, so drop it with the "app's" one.
        let mut child = handed(&lock).spawn().unwrap();
        drop(lock);
        std::thread::sleep(Duration::from_secs(1));
        eprintln!(
            "=== {signal}: after 1 s: running {:?}, a second app can lock: {}",
            processes(),
            lock_is_free(lock_path)
        );
        if signal == "none" {
            let status = child.wait().unwrap();
            eprintln!(
                "normal refresh with the folder as stdin: exit {:?}, list arrived: {}",
                status.code(),
                Path::new(&private_list()).is_file()
            );
            assert!(status.success() && Path::new(&private_list()).is_file());
        } else {
            let _ = Command::new("/usr/bin/kill")
                .args([signal, &child.id().to_string()])
                .status();
            std::thread::sleep(Duration::from_secs(1));
            let _ = child.try_wait();
            eprintln!(
                "started process killed: running {:?}, a second app can lock: {}",
                processes(),
                lock_is_free(lock_path)
            );
            assert!(!lock_is_free(lock_path), "the orphan pacman lost the lock");
            for line in processes() {
                if line.ends_with(" pacman") {
                    let pid = line.split(' ').next().unwrap().to_string();
                    let _ = Command::new("/usr/bin/kill").args(["-KILL", &pid]).status();
                }
            }
            std::thread::sleep(Duration::from_secs(1));
            eprintln!(
                "pacmans gone: running {:?}, a second app can lock: {}",
                processes(),
                lock_is_free(lock_path)
            );
            assert!(
                lock_is_free(lock_path),
                "something else still holds the lock"
            );
        }
        kill_all_found();
        let _ = child.wait();
    }
    let _ = fs::remove_file(&db);
    fs::copy("/tmp/original-test-repo.db", &db).unwrap();
}
