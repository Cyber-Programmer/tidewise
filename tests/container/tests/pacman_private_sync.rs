//! What real pacman does with a private copy of the package lists (WBS 6.10b, decision 0007). These are measurements,
//! green from the start, run before any helper code depends on them: the helper's update flow syncs a private
//! `--dbpath` in `/run/tidewise-helper-lists/` and only then moves the verified lists into `/var/lib/pacman/sync/`.
//!
//! Real pacman, as root, inside the Tidewise test container only (`tests/container/run.sh`).

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Output};

mod common;
use common::*;

const LATER_PACKAGE: &str = "tidewise-dummy-upd-2.0-1-any.pkg.tar.zst";

/// `pacman <args> --dbpath <dbpath> --logfile /dev/null`.
fn private_pacman(dbpath: &str, args: &[&str]) -> Output {
    let mut all: Vec<&str> = args.to_vec();
    all.extend(["--dbpath", dbpath, "--logfile", "/dev/null"]);
    pacman(&all)
}

fn text(output: &Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// "owner:group mode" of `path`, not following a link.
fn owner_and_mode(path: &str) -> String {
    let meta = fs::symlink_metadata(path).unwrap();
    format!("{}:{} {:o}", meta.uid(), meta.gid(), meta.mode() & 0o7777)
}

fn start() {
    assert_inside_the_test_container();
    reset_world();
    let _ = fs::remove_dir_all(PRIVATE_LISTS);
    let _ = fs::remove_dir_all("/run/tw-lists-0700");
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn the_download_as_alpm_works_through_a_0711_folder_and_fails_through_a_0700_one() {
    start();

    private_folder(PRIVATE_LISTS, 0o711);
    let through_0711 = private_pacman(PRIVATE_LISTS, &["-Sy"]);
    assert!(through_0711.status.success(), "{}", text(&through_0711));
    assert!(Path::new(&format!("{PRIVATE_LISTS}sync/tidewise-test.db")).is_file());

    private_folder("/run/tw-lists-0700/", 0o700);
    let through_0700 = private_pacman("/run/tw-lists-0700/", &["-Sy"]);
    eprintln!("through 0700: {}", text(&through_0700));
    assert!(!through_0700.status.success(), "{}", text(&through_0700));
    assert!(!Path::new("/run/tw-lists-0700/sync/tidewise-test.db").exists());
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_finished_list_is_owned_by_root_with_mode_0644_in_the_private_and_in_the_real_folder() {
    start();
    private_folder(PRIVATE_LISTS, 0o711);

    let private = private_pacman(PRIVATE_LISTS, &["-Sy"]);
    assert!(private.status.success(), "{}", text(&private));
    let real = pacman(&["-Syy"]);
    assert!(real.status.success(), "{}", text(&real));

    let private_list = format!("{PRIVATE_LISTS}sync/tidewise-test.db");
    let real_list = format!("{REAL_SYNC}/tidewise-test.db");
    eprintln!(
        "private {} / real {} / private sync dir {} / real sync dir {} / umask of this test {:?}",
        owner_and_mode(&private_list),
        owner_and_mode(&real_list),
        owner_and_mode(&format!("{PRIVATE_LISTS}sync")),
        owner_and_mode(REAL_SYNC),
        fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|l| l.starts_with("Umask"))
    );
    assert_eq!(owner_and_mode(&private_list), "0:0 644");
    assert_eq!(owner_and_mode(&real_list), "0:0 644");
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn logfile_dev_null_keeps_the_private_sync_out_of_pacman_log() {
    start();
    private_folder(PRIVATE_LISTS, 0o711);
    let before = pacman_log();

    let private = private_pacman(PRIVATE_LISTS, &["-Sy"]);
    assert!(private.status.success(), "{}", text(&private));
    let dry = private_pacman(PRIVATE_LISTS, &["-Su", "--print"]);
    assert!(dry.status.success(), "{}", text(&dry));
    assert_eq!(pacman_log(), before, "the private sync wrote to pacman.log");

    // The control: the same sync on the real lists does write there, so the test above can fail.
    let real = pacman(&["-Syy"]);
    assert!(real.status.success(), "{}", text(&real));
    assert!(
        pacman_log().contains("synchronizing package lists"),
        "the control did not log"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_private_sync_changes_nothing_outside_the_private_folder() {
    start();
    private_folder(PRIVATE_LISTS, 0o711);
    change_repository_without_syncing(LATER_PACKAGE);

    let lists_before = fingerprint(REAL_SYNC);
    let log_before = pacman_log();
    let local_before = pacman_stdout(&["-Q"]);

    let private = private_pacman(PRIVATE_LISTS, &["-Sy"]);
    assert!(private.status.success(), "{}", text(&private));

    assert_eq!(
        fingerprint(REAL_SYNC),
        lists_before,
        "the real lists changed"
    );
    assert_eq!(pacman_log(), log_before, "pacman.log changed");
    assert_eq!(pacman_stdout(&["-Q"]), local_before);
    assert!(
        !Path::new("/var/lib/pacman/db.lck").exists(),
        "the real db.lck appeared"
    );
    // And the private sync did fetch something different, or the test above proves nothing.
    assert_ne!(
        sha256(&format!("{PRIVATE_LISTS}sync/tidewise-test.db")),
        sha256(&format!("{REAL_SYNC}/tidewise-test.db"))
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_stale_private_lock_stops_the_private_sync_and_a_dry_run_does_not_look_at_it() {
    start();
    private_folder(PRIVATE_LISTS, 0o711);
    let first = private_pacman(PRIVATE_LISTS, &["-Sy"]);
    assert!(first.status.success(), "{}", text(&first));
    change_repository_without_syncing(LATER_PACKAGE);
    fs::write(format!("{PRIVATE_LISTS}db.lck"), b"").unwrap();

    let lists_before = fingerprint(REAL_SYNC);
    let lists_in = |dir: &str| -> Vec<String> {
        fingerprint(dir)
            .into_iter()
            .filter(|line| !line.starts_with("download-"))
            .collect()
    };
    let private_before = lists_in(&format!("{PRIVATE_LISTS}sync"));
    let locked = private_pacman(PRIVATE_LISTS, &["-Sy"]);
    eprintln!("stale private lock: {}", text(&locked));
    assert!(!locked.status.success());
    assert!(
        String::from_utf8_lossy(&locked.stderr).contains("unable to lock database"),
        "{}",
        text(&locked)
    );
    assert!(
        Path::new(&format!("{PRIVATE_LISTS}db.lck")).exists(),
        "pacman removed the lock"
    );
    // The lists did not change. pacman makes its empty `download-XXXXXX` folder before it asks for the lock, so one
    // is left behind in the private `sync/` (it is empty, it is on tmpfs, the helper never reads or deletes it).
    assert_eq!(lists_in(&format!("{PRIVATE_LISTS}sync")), private_before);
    let leftovers = fingerprint(&format!("{PRIVATE_LISTS}sync"))
        .into_iter()
        .filter(|line| line.starts_with("download-"))
        .count();
    assert_eq!(leftovers, 1, "one empty download folder is left behind");
    assert_eq!(fingerprint(REAL_SYNC), lists_before);
    assert!(!Path::new("/var/lib/pacman/db.lck").exists());

    // A dry run takes no lock (0007, fact 4).
    let dry = private_pacman(PRIVATE_LISTS, &["-Su", "--print"]);
    assert!(dry.status.success(), "{}", text(&dry));
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn the_dbpath_works_with_and_without_the_trailing_slash() {
    start();
    private_folder(PRIVATE_LISTS, 0o711);
    let with = private_pacman(PRIVATE_LISTS, &["-Sy"]);
    assert!(with.status.success(), "{}", text(&with));
    let without = private_pacman(PRIVATE_LISTS.trim_end_matches('/'), &["-Syy"]);
    assert!(without.status.success(), "{}", text(&without));
    assert!(Path::new(&format!("{PRIVATE_LISTS}sync/tidewise-test.db")).is_file());
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn pacman_dash_sc_removes_a_leftover_staged_file_and_a_download_folder_but_keeps_the_lists() {
    start();
    fs::write(format!("{REAL_SYNC}/.tidewise-core.db.new"), b"half").unwrap();
    fs::create_dir_all(format!("{REAL_SYNC}/download-AbCdEf")).unwrap();
    // core and extra are not configured in this container (`local_repository_only`), so `-Sc` removes their lists as
    // "unused repositories": only the configured repository's list is compared.
    let kept_list = |lines: Vec<String>| -> Vec<String> {
        lines
            .into_iter()
            .filter(|line| line.starts_with("tidewise-test.db "))
            .collect()
    };
    let lists = kept_list(fingerprint(REAL_SYNC));
    assert_eq!(lists.len(), 1);

    let clean = pacman(&["-Sc", "--noconfirm"]);
    eprintln!("-Sc: {}", text(&clean));
    assert!(clean.status.success(), "{}", text(&clean));
    assert!(!Path::new(&format!("{REAL_SYNC}/.tidewise-core.db.new")).exists());
    assert!(!Path::new(&format!("{REAL_SYNC}/download-AbCdEf")).exists());
    assert_eq!(kept_list(fingerprint(REAL_SYNC)), lists);
}

// ---- signatures (change 1 of the 6.10b plan) --------------------------------------------------------------

const SIG_HOME: &str = "/tmp/tw-sig";

/// A pacman.conf for the one local repository with signatures optional but checked when present, and its own GnuPG
/// folder. Its database folder is `<SIG_HOME>/db/`.
fn signed_world() -> String {
    let _ = fs::remove_dir_all(SIG_HOME);
    folder(SIG_HOME, 0o711);
    folder(&format!("{SIG_HOME}/gnupg"), 0o700);
    private_folder(&format!("{SIG_HOME}/db"), 0o711);
    let conf = format!(
        "[options]\nDBPath = {SIG_HOME}/db/\nLogFile = /dev/null\nGPGDir = {SIG_HOME}/gnupg\nSigLevel = Optional TrustAll\n\
         DownloadUser = alpm\nArchitecture = auto\n\n[tidewise-test]\nServer = file:///srv/tidewise-test\n"
    );
    let path = format!("{SIG_HOME}/pacman.conf");
    fs::write(&path, conf).unwrap();
    let made = Command::new("/usr/bin/gpg")
        .args(["--homedir", &format!("{SIG_HOME}/gnupg"), "--batch"])
        .args(["--pinentry-mode", "loopback", "--passphrase", ""])
        .args([
            "--quick-gen-key",
            "tidewise-test-signer",
            "default",
            "default",
            "never",
        ])
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    path
}

fn sign_repository() {
    let signed = Command::new("/usr/bin/gpg")
        .args([
            "--homedir",
            &format!("{SIG_HOME}/gnupg"),
            "--batch",
            "--yes",
        ])
        .args([
            "--detach-sign",
            "-o",
            &format!("{REPO_DIR}/tidewise-test.db.sig"),
        ])
        .arg(format!("{REPO_DIR}/tidewise-test.db.tar.gz"))
        .output()
        .unwrap();
    assert!(
        signed.status.success(),
        "{}",
        String::from_utf8_lossy(&signed.stderr)
    );
    fs::set_permissions(
        format!("{REPO_DIR}/tidewise-test.db.sig"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn pacman_leaves_a_stale_signature_next_to_a_new_unsigned_list() {
    start();
    let conf = signed_world();
    let db = format!("{SIG_HOME}/db/");
    let with_config = |args: &[&str]| -> Output {
        let mut all = vec!["--config", conf.as_str()];
        all.extend_from_slice(args);
        pacman(&all)
    };

    // 1. A signed repository: the list and its .sig arrive, root-owned.
    sign_repository();
    let signed = with_config(&["-Sy"]);
    eprintln!("signed sync: {}", text(&signed));
    assert!(signed.status.success(), "{}", text(&signed));
    let sig = format!("{db}sync/tidewise-test.db.sig");
    assert!(Path::new(&sig).is_file(), "no .sig after a signed sync");
    eprintln!("sig owner/mode: {}", owner_and_mode(&sig));
    let first_sig = sha256(&sig);
    let first_list = sha256(&format!("{db}sync/tidewise-test.db"));

    // 2. The repository stops being signed AND changes: a new list arrives with no .sig.
    fs::remove_file(format!("{REPO_DIR}/tidewise-test.db.sig")).unwrap();
    change_repository_without_syncing(LATER_PACKAGE);
    let unsigned = with_config(&["-Syy"]);
    let invalid = "invalid or corrupted database (PGP signature)";
    // pacman's own sync downloads the new list, does NOT remove the stale .sig, and then fails on the pair.
    assert!(!unsigned.status.success(), "{}", text(&unsigned));
    assert!(text(&unsigned).contains(invalid), "{}", text(&unsigned));
    assert_ne!(
        sha256(&format!("{db}sync/tidewise-test.db")),
        first_list,
        "the new list did not arrive"
    );
    assert!(Path::new(&sig).is_file(), "pacman removed the stale .sig");
    assert_eq!(sha256(&sig), first_sig, "pacman changed the stale .sig");

    // Every later run on that folder fails the same way, a dry run included: the pair stays invalid until a person
    // deals with the .sig. A move that leaves the .sig alone matches this exactly (0007).
    for args in [&["-Su", "--print"][..], &["-Syy"][..]] {
        let next = with_config(args);
        assert!(!next.status.success(), "{}", text(&next));
        assert!(text(&next).contains(invalid), "{}", text(&next));
        assert!(Path::new(&sig).is_file());
    }
}
