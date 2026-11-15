//! The helper's jobs, end to end: the real `tidewise-helper` as real root, running real pacman, inside the Tidewise test
//! container (WBS 5.3b). Every success path and every way a job can stop, with the journal, the events on stdout, and
//! what changed on the system (or, for a refusal, that nothing did).
//!
//! The requests are built the way the app builds them: the confirmed list is what `pacman --print` says right now. The
//! packages are the dummies of `tests/container/pkgs`; newer versions appear with `add_newer` (copy, `repo-add`,
//! `pacman -Syy`), so a real update, a real replacement and a real "the repository changed after the list was made" all
//! happen offline.
//!
//! Real pacman, as root: only inside the test container (`tests/container/run.sh`). Each test starts with the guard and
//! `reset_world()`.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

mod common;
use common::*;

const LOCK: &str = "/var/lib/pacman/db.lck";
const OK_LINE: &str = "{\"finished\":\"ok\"}";

fn qualified(name: &str) -> String {
    format!("{REPO}/tidewise-dummy-{name}")
}

fn dummy(name: &str) -> String {
    format!("tidewise-dummy-{name}")
}

/// An install request for one package, with the list `pacman --print` gives right now.
fn install_one(name: &str) -> (String, String) {
    let items = confirmed("-S", &[&qualified(name)]);
    install_request(&[(REPO, &dummy(name))], &[], &items)
}

fn assert_ok(ran: &Ran) {
    assert_eq!(ran.status.code(), Some(0), "{}\n{}", ran.stdout, ran.stderr);
    assert_eq!(ran.lines().last(), Some(&OK_LINE), "{}", ran.stdout);
    assert_eq!(ran.stderr, "");
}

fn finished_with(ran: &Ran, code: &str) {
    assert_eq!(ran.status.code(), Some(1), "{}", ran.stdout);
    let last = ran.lines().last().copied().unwrap_or_default();
    let error_line = format!("{{\"finished\":{{\"error\":\"{code}\"}}}}");
    let error_with_details = format!("{{\"finished\":{{\"error\":{{\"{code}\":");
    assert!(
        last == error_line || last.starts_with(&error_with_details),
        "{last}"
    );
}

// ---- success paths --------------------------------------------------------------------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_clean_install_runs_and_leaves_one_package_two_journal_entries_and_a_tidy_work_folder() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    let (request, summary) = install_one("hello");

    let ran = run_with(&[&summary], &[("PKEXEC_UID", "1000")], request.as_bytes());
    assert_ok(&ran);
    assert!(
        ran.lines()[0].starts_with("{\"progress\":"),
        "{}",
        ran.stdout
    );
    assert_eq!(
        installed_version("tidewise-dummy-hello").as_deref(),
        Some("1.0-1")
    );
    assert_eq!(
        install_reason("tidewise-dummy-hello"),
        "Explicitly installed"
    );
    // pacman's own log records the change too, independently of the helper's journal entries.
    assert!(pacman_log().contains("installed tidewise-dummy-hello (1.0-1)"));

    let entries = journal.datagrams();
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert_fields(
        &entries[0],
        &[
            ("SYSLOG_IDENTIFIER", "tidewise-helper"),
            ("PRIORITY", "6"),
            ("TIDEWISE_CALLER_UID", "1000"),
            ("TIDEWISE_STAGE", "accepted"),
            ("TIDEWISE_SUMMARY", &summary),
        ],
    );
    assert_fields(
        &entries[1],
        &[
            ("PRIORITY", "6"),
            ("TIDEWISE_STAGE", "finished"),
            ("TIDEWISE_OUTCOME", "ok"),
            ("TIDEWISE_SUMMARY", &summary),
        ],
    );
    // A clean ok carries no notes: a check that passed is not announced.
    for field in [
        "NOT_INSTALLED",
        "REMOVED",
        "REMOVAL_CHECK",
        "ok_with_notices",
    ] {
        assert!(!entries[1].contains(field), "{}", entries[1]);
    }

    // The work folder exists, is root's and 0700, and the run's files are gone.
    assert_eq!(mode_of(WORK_DIR), 0o700);
    assert_eq!(fs::metadata(WORK_DIR).unwrap().uid(), 0);
    assert_eq!(fs::read_dir(WORK_DIR).unwrap().count(), 0);
    assert!(!Path::new(LOCK).exists());
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_chosen_provider_is_installed_as_a_dependency_and_the_target_is_not() {
    assert_inside_the_test_container();
    reset_world();
    let names = [qualified("needs-virtual"), qualified("prov-b")];
    let items = confirmed("-S", &[&names[0], &names[1]]);
    let (request, summary) = install_request(
        &[(REPO, "tidewise-dummy-needs-virtual")],
        &[("tidewise-virtual", REPO, "tidewise-dummy-prov-b")],
        &items,
    );
    assert_eq!(items.len(), 2);

    let ran = run(&summary, request.as_bytes());
    assert_ok(&ran);
    assert_eq!(
        installed_version("tidewise-dummy-prov-b").as_deref(),
        Some("1.0-1")
    );
    assert_eq!(installed_version("tidewise-dummy-prov-a"), None);
    assert_eq!(
        install_reason("tidewise-dummy-prov-b"),
        "Installed as a dependency for another package"
    );
    assert_eq!(
        install_reason("tidewise-dummy-needs-virtual"),
        "Explicitly installed"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_provider_the_user_already_installed_by_hand_stays_explicit() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-prov-b"]);
    assert_eq!(
        install_reason("tidewise-dummy-prov-b"),
        "Explicitly installed"
    );
    let names = [qualified("needs-virtual"), qualified("prov-b")];
    let items = confirmed("-S", &[&names[0], &names[1]]);
    let (request, summary) = install_request(
        &[(REPO, "tidewise-dummy-needs-virtual")],
        &[("tidewise-virtual", REPO, "tidewise-dummy-prov-b")],
        &items,
    );

    assert_ok(&run(&summary, request.as_bytes()));
    assert_eq!(
        installed_version("tidewise-dummy-needs-virtual").as_deref(),
        Some("1.0-1")
    );
    // The user's own package was not demoted to "installed as a dependency" (review R-02).
    assert_eq!(
        install_reason("tidewise-dummy-prov-b"),
        "Explicitly installed"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_system_update_updates_the_package_and_syncs_exactly_once() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-upd"]);
    add_newer("tidewise-dummy-upd-2.0-1-any.pkg.tar.zst");
    let items = confirmed("-Su", &[]);
    assert_eq!(items.len(), 1);
    let (request, summary) = update_request(&items);

    let log_before = pacman_log();
    let ran = run(&summary, request.as_bytes());
    assert_ok(&ran);
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("2.0-1")
    );
    let new_log = &pacman_log()[log_before.len()..];
    let syncs = new_log
        .lines()
        .filter(|l| l.contains("synchronizing package lists"))
        .count();
    assert_eq!(syncs, 1, "one sync, not two:\n{new_log}");
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_and_an_install_in_one_request_do_both() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-upd"]);
    add_newer("tidewise-dummy-upd-2.0-1-any.pkg.tar.zst");
    let items = confirmed("-Su", &[&qualified("hello")]);
    assert_eq!(items.len(), 2);
    let (request, summary) =
        update_and_install_request(&[(REPO, "tidewise-dummy-hello")], &[], &items);

    let ran = run(&summary, request.as_bytes());
    assert_ok(&ran);
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("2.0-1")
    );
    assert_eq!(
        installed_version("tidewise-dummy-hello").as_deref(),
        Some("1.0-1")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_pending_replacement_is_declined_and_reported_as_a_notice_not_an_error() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start(); // with a journal, the only notice is the one about the packages
    install_directly(&["tidewise-dummy-old"]);
    add_newer("tidewise-dummy-new-1.0-1-any.pkg.tar.zst");
    // The dry run lists the replacing package (and says nothing about the removal), so that is the confirmed list.
    let items = confirmed("-Su", &[]);
    assert_eq!(items.len(), 1);
    let (request, summary) = update_request(&items);

    let ran = run(&summary, request.as_bytes());
    assert_eq!(ran.status.code(), Some(0), "{}", ran.stdout);
    let lines = ran.lines();
    let n = lines.len();
    assert_eq!(lines[n - 2], "{\"notice\":{\"packages_not_installed\":1}}");
    assert_eq!(lines[n - 1], OK_LINE);
    // Nothing was removed and the replacement was not installed.
    assert_eq!(
        installed_version("tidewise-dummy-old").as_deref(),
        Some("1.0-1")
    );
    assert_eq!(installed_version("tidewise-dummy-new"), None);

    // The journal does not call that a clean ok (5.3c): the same fact, one line, with its count.
    let entries = journal.datagrams();
    assert_eq!(entries.len(), 2);
    assert_fields(
        &entries[1],
        &[
            ("PRIORITY", "4"),
            ("TIDEWISE_OUTCOME", "ok_with_notices"),
            ("TIDEWISE_NOT_INSTALLED", "1"),
        ],
    );
    assert!(
        entries[1].contains("(1 not installed, 0 removed)"),
        "{}",
        entries[1]
    );
    assert!(
        !entries[1].contains("REMOVAL_CHECK"),
        "the check ran: {}",
        entries[1]
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_package_that_disappears_during_the_run_is_reported_as_removed() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    // The victim is installed. Installing the remover makes the victim vanish from `pacman -Q` (its install script
    // deletes the victim's database entry): something other than the request removed a package.
    install_directly(&["tidewise-dummy-victim"]);
    let (request, summary) = install_one("remover");

    let ran = run(&summary, request.as_bytes());
    assert_eq!(ran.status.code(), Some(0), "{}", ran.stdout);
    let lines = ran.lines();
    let n = lines.len();
    assert_eq!(lines[n - 2], "{\"notice\":{\"packages_removed\":1}}");
    assert_eq!(lines[n - 1], OK_LINE);
    assert_eq!(
        installed_version("tidewise-dummy-remover").as_deref(),
        Some("1.0-1")
    );
    assert_eq!(installed_version("tidewise-dummy-victim"), None);

    let entries = journal.datagrams();
    assert_eq!(entries.len(), 2);
    assert_fields(
        &entries[1],
        &[
            ("PRIORITY", "4"),
            ("TIDEWISE_STAGE", "finished"),
            ("TIDEWISE_OUTCOME", "ok_with_notices"),
            ("TIDEWISE_REMOVED", "1"),
        ],
    );
    assert!(
        entries[1].contains("(0 not installed, 1 removed)"),
        "{}",
        entries[1]
    );
    assert!(!entries[1].contains("REMOVAL_CHECK"), "{}", entries[1]);
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_removal_check_that_cannot_read_the_list_is_logged_as_skipped_never_as_a_clean_ok() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    // The breaker's install script removes the database's version file, so the second `pacman -Q` fails (exit 255).
    let (request, summary) = install_one("breaker");
    let ran = run(&summary, request.as_bytes());
    // The test puts the file back before anything else needs pacman.
    fs::copy(
        "/tmp/tidewise-db-version-backup",
        "/var/lib/pacman/local/ALPM_DB_VERSION",
    )
    .unwrap();

    // The job itself went fine and says so; there is nothing to report as a notice (the list could not be read) ...
    assert_ok(&ran);
    assert!(!ran.stdout.contains("notice"), "{}", ran.stdout);
    assert_eq!(
        installed_version("tidewise-dummy-breaker").as_deref(),
        Some("1.0-1")
    );
    // ... but the journal does not call it a clean pass.
    let entries = journal.datagrams();
    assert_eq!(entries.len(), 2);
    assert_fields(
        &entries[1],
        &[
            ("PRIORITY", "4"),
            ("TIDEWISE_OUTCOME", "ok_with_notices"),
            ("TIDEWISE_REMOVAL_CHECK", "skipped"),
        ],
    );
    assert!(
        entries[1].contains("removal check skipped"),
        "{}",
        entries[1]
    );
}

// ---- ways a job stops -----------------------------------------------------------------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_held_lock_is_database_locked_the_lock_is_untouched_and_pacman_does_not_run() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    let (request, summary) = install_one("hello");
    fs::write(LOCK, b"").unwrap();
    let lock_before = fs::metadata(LOCK).unwrap();
    let log_before = pacman_log();
    let files_before = snapshot();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "database_locked");
    // pacman's advice to remove the lock is never repeated (golden rule 9).
    assert!(
        !ran.stdout.contains("remove") && !ran.stdout.contains("db.lck"),
        "{}",
        ran.stdout
    );
    let lock_after = fs::metadata(LOCK).unwrap();
    assert_eq!(
        (
            lock_before.ino(),
            lock_before.mtime(),
            lock_before.mtime_nsec()
        ),
        (
            lock_after.ino(),
            lock_after.mtime(),
            lock_after.mtime_nsec()
        )
    );
    assert_eq!(pacman_log(), log_before, "pacman must not have run");
    let changed = differences(&files_before, &snapshot());
    assert!(changed.is_empty(), "these paths changed: {changed:?}");
    assert_eq!(installed_version("tidewise-dummy-hello"), None);

    let entries = journal.datagrams();
    assert_eq!(entries.len(), 2);
    assert_fields(
        &entries[1],
        &[("PRIORITY", "4"), ("TIDEWISE_OUTCOME", "database_locked")],
    );
    let _ = fs::remove_file(LOCK); // the test container's hygiene: the helper never does this
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_pending_update_blocks_a_plain_install_and_nothing_changes() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-upd"]);
    add_newer("tidewise-dummy-upd-2.0-1-any.pkg.tar.zst");
    let (request, summary) = install_one("hello");
    let packages_before = package_list();
    let files_before = snapshot();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "system_out_of_date");
    assert_eq!(package_list(), packages_before);
    let changed = differences(&files_before, &snapshot());
    assert!(changed.is_empty(), "these paths changed: {changed:?}");
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_list_that_is_not_what_pacman_says_is_a_changed_transaction_and_nothing_changes() {
    assert_inside_the_test_container();
    reset_world();
    let mut items = confirmed("-S", &[&qualified("hello")]);
    items[0].version = "9.9-9".to_string();
    let (request, summary) = install_request(&[(REPO, "tidewise-dummy-hello")], &[], &items);
    let packages_before = package_list();
    let files_before = snapshot();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "transaction_changed");
    assert_eq!(package_list(), packages_before);
    let changed = differences(&files_before, &snapshot());
    assert!(changed.is_empty(), "these paths changed: {changed:?}");
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_repository_that_changed_after_the_list_was_made_is_a_changed_transaction() {
    assert_inside_the_test_container();
    reset_world();
    // The user confirms upd 1.0 ...
    let items = confirmed("-S", &[&qualified("upd")]);
    assert_eq!(items[0].version, "1.0-1");
    let (request, summary) = install_request(&[(REPO, "tidewise-dummy-upd")], &[], &items);
    // ... and before the helper runs, the repository gets 2.0.
    add_newer("tidewise-dummy-upd-2.0-1-any.pkg.tar.zst");
    let packages_before = package_list();
    let files_before = snapshot();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "transaction_changed");
    assert_eq!(installed_version("tidewise-dummy-upd"), None);
    assert_eq!(package_list(), packages_before);
    let changed = differences(&files_before, &snapshot());
    assert!(changed.is_empty(), "these paths changed: {changed:?}");
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_conflict_aborts_with_pacmans_words_installs_nothing_and_the_notice_counts_the_list() {
    assert_inside_the_test_container();
    reset_world();
    let _journal = FakeJournal::start(); // with a journal, the only notice is the one about the packages
    install_directly(&["tidewise-dummy-conf-x"]);
    // The dry run does not notice the conflict, so the list is "confirmed" ...
    let (request, summary) = install_one("conf-y");

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "pacman_failed");
    // ... and the real run aborts, with pacman's own words under the technical details.
    assert!(
        ran.stdout
            .contains("unresolvable package conflicts detected"),
        "{}",
        ran.stdout
    );
    assert_eq!(installed_version("tidewise-dummy-conf-y"), None);
    assert_eq!(
        installed_version("tidewise-dummy-conf-x").as_deref(),
        Some("1.0-1")
    );
    // The post-check: the one confirmed package is not installed.
    let lines = ran.lines();
    assert_eq!(
        lines[lines.len() - 2],
        "{\"notice\":{\"packages_not_installed\":1}}"
    );
}

// ---- the work folder ------------------------------------------------------------------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn only_the_helpers_own_leftovers_of_dead_processes_are_removed() {
    assert_inside_the_test_container();
    reset_world();
    fs::create_dir(WORK_DIR).unwrap();
    fs::set_permissions(
        WORK_DIR,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    for name in [
        "tidewise-helper-99999999-0.out", // a process that cannot exist: removed
        "tidewise-helper-99999999-0.err",
        "tidewise-helper-1-0.out", // process 1 is alive: kept
        "other.txt",               // not ours: kept
        "tidewise-helper-99999999-0.out.bak",
    ] {
        fs::write(format!("{WORK_DIR}/{name}"), b"x").unwrap();
    }
    let (request, summary) = install_one("hello");
    assert_ok(&run(&summary, request.as_bytes()));

    let mut left: Vec<String> = fs::read_dir(WORK_DIR)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(
        left,
        vec![
            "other.txt",
            "tidewise-helper-1-0.out",
            "tidewise-helper-99999999-0.out.bak"
        ]
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_work_folder_that_is_not_what_it_should_be_refuses_the_job_and_runs_nothing() {
    assert_inside_the_test_container();
    reset_world();
    let (request, summary) = install_one("hello");
    let log_before = pacman_log();

    // A folder with the wrong mode: refused, not repaired.
    fs::create_dir(WORK_DIR).unwrap();
    fs::set_permissions(
        WORK_DIR,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "pacman_failed");
    assert!(
        ran.stdout.contains("work folder could not be used"),
        "{}",
        ran.stdout
    );
    assert_eq!(mode_of(WORK_DIR), 0o755, "it must not be repaired");

    // A symlink: refused, not followed, nothing written through it.
    fs::remove_dir(WORK_DIR).unwrap();
    fs::create_dir("/tmp/tidewise-elsewhere").unwrap();
    std::os::unix::fs::symlink("/tmp/tidewise-elsewhere", WORK_DIR).unwrap();
    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "pacman_failed");
    assert_eq!(fs::read_dir("/tmp/tidewise-elsewhere").unwrap().count(), 0);
    assert!(
        fs::symlink_metadata(WORK_DIR)
            .unwrap()
            .file_type()
            .is_symlink()
    );

    assert_eq!(pacman_log(), log_before, "pacman must not have run");
    assert_eq!(installed_version("tidewise-dummy-hello"), None);
    let _ = fs::remove_file(WORK_DIR);
    let _ = fs::remove_dir("/tmp/tidewise-elsewhere");
}

// ---- environment and umask ------------------------------------------------------------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_install_script_sees_exactly_path_and_lc_all_and_a_command_from_path_works() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    let (request, summary) = install_one("scriptlet");
    let hostile = [
        ("PKEXEC_UID", "0\nMESSAGE=pwned"),
        ("FOO", "bar"),
        ("PATH", "/tmp/evil"),
        ("HOME", "/tmp/evil"),
        ("PACMAN_ROOT", "/tmp/evil"),
        ("LC_ALL", "tr_TR.UTF-8"),
        ("IFS", "x"),
    ];
    let ran = run_with(&[&summary], &hostile, request.as_bytes());
    assert_ok(&ran);

    let env = fs::read_to_string("/tmp/tidewise-scriptlet-env").unwrap();
    let keys: Vec<&str> = env
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, _)| k))
        .collect();
    assert!(env.lines().any(|l| l == "PATH=/usr/bin"), "{env}");
    assert!(env.lines().any(|l| l == "LC_ALL=C"), "{env}");
    assert!(
        env.lines().any(|l| l == "PWD=/"),
        "the scriptlet runs in /: {env}"
    );
    for key in &keys {
        // Only what we set, plus what pacman and the shell add themselves.
        assert!(
            ["PATH", "LC_ALL", "HTTP_USER_AGENT", "PWD", "SHLVL", "_"].contains(key),
            "the install script saw {key}: {env}"
        );
    }
    // A command found through PATH worked, and the scriptlet's umask is the one pacman gives scriptlets.
    assert_eq!(
        fs::read_to_string("/tmp/tidewise-scriptlet-uname")
            .unwrap()
            .trim(),
        "Linux"
    );
    assert_eq!(
        fs::read_to_string("/tmp/tidewise-scriptlet-umask")
            .unwrap()
            .trim(),
        "0022"
    );

    let entries = journal.datagrams();
    assert_eq!(entries.len(), 2);
    assert_fields(&entries[0], &[("TIDEWISE_CALLER_UID", "unknown")]);
    assert!(!entries[0].contains("pwned") && !entries[1].contains("pwned"));
}

fn assert_modes_of(package: &str) {
    let name = format!("tidewise-dummy-{package}");
    assert_eq!(
        mode_of(&format!(
            "/var/cache/pacman/pkg/{name}-1.0-1-any.pkg.tar.zst"
        )),
        0o644,
        "the downloaded package file"
    );
    assert_eq!(
        mode_of(&format!("/usr/share/tidewise-test/{name}.txt")),
        0o644,
        "the installed file"
    );
    assert_eq!(
        mode_of(&format!("/var/lib/pacman/local/{name}-1.0-1")),
        0o755,
        "the database entry"
    );
    assert_eq!(
        mode_of(&format!("/var/lib/pacman/local/{name}-1.0-1/desc")),
        0o644,
        "its desc file"
    );
    assert_eq!(
        mode_of(&format!("/var/lib/pacman/local/{name}-1.0-1/files")),
        0o644,
        "its files file"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn whatever_umask_the_helper_is_started_with_the_files_come_out_as_with_022() {
    assert_inside_the_test_container();
    reset_world();
    // Without the helper's own umask(022), pacman's downloaded package file followed the caller's umask: 600 under 077
    // and 666 (world-writable) under 000 (measured).
    for (mask, package) in [("077", "hello"), ("000", "lib")] {
        let (request, summary) = install_one(package);
        let ran = run_under_umask(mask, &summary, request.as_bytes());
        assert_ok(&ran);
        assert_modes_of(package);
    }
}

// ---- the caller goes away -------------------------------------------------------------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_caller_that_goes_away_in_the_middle_of_an_install_does_not_stop_it() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    let (request, summary) = install_one("slow-a");
    let (mut child, writer) = spawn_job(&summary, &request, false);
    assert!(
        wait_for_file("/tmp/tidewise-slow-a-started", Duration::from_secs(30)),
        "the install never started"
    );

    // The app disappears: nobody reads the helper's stdout any more.
    drop(child.stdout.take());
    let status =
        wait_up_to(&mut child, Duration::from_secs(60)).expect("the helper did not finish");
    writer.join().unwrap();
    assert_eq!(status.code(), Some(0), "{status:?}");
    assert!(Path::new("/tmp/tidewise-slow-a-done").exists());
    assert_eq!(
        installed_version("tidewise-dummy-slow-a").as_deref(),
        Some("1.0-1")
    );
    assert!(!Path::new(LOCK).exists());
    let entries = journal.datagrams();
    assert_eq!(entries.len(), 2);
    assert_fields(&entries[1], &[("TIDEWISE_OUTCOME", "ok")]);
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_hangup_that_kills_the_helper_mid_install_does_not_stop_pacman() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    // Two targets with slow install scripts (so a hangup that reached pacman would stop it between them), and a target
    // that needs a chosen provider (so the -D step exists).
    let names = [
        qualified("slow-a"),
        qualified("slow-b"),
        qualified("needs-virtual"),
        qualified("prov-b"),
    ];
    let items = confirmed("-S", &[&names[0], &names[1], &names[2], &names[3]]);
    let (request, summary) = install_request(
        &[
            (REPO, "tidewise-dummy-slow-a"),
            (REPO, "tidewise-dummy-slow-b"),
            (REPO, "tidewise-dummy-needs-virtual"),
        ],
        &[("tidewise-virtual", REPO, "tidewise-dummy-prov-b")],
        &items,
    );
    let (mut child, writer) = spawn_job(&summary, &request, true);
    let started = wait_for_file("/tmp/tidewise-slow-a-started", Duration::from_secs(30))
        || wait_for_file("/tmp/tidewise-slow-b-started", Duration::from_secs(1));
    assert!(started, "the install never started");

    // SIGHUP to the helper's process group (what closing a terminal does). pacman is in its own group.
    let pid = child.id();
    let killed = std::process::Command::new("/usr/bin/kill")
        .args(["-s", "HUP", "--", &format!("-{pid}")])
        .status()
        .unwrap();
    assert!(killed.success());
    let status = wait_up_to(&mut child, Duration::from_secs(30)).expect("the helper did not stop");
    writer.join().unwrap();
    assert_eq!(
        status.signal(),
        Some(1),
        "the helper dies of the hangup: {status:?}"
    );

    // pacman was not hit: it finishes the whole transaction and releases its lock.
    for marker in ["/tmp/tidewise-slow-a-done", "/tmp/tidewise-slow-b-done"] {
        assert!(
            wait_for_file(marker, Duration::from_secs(30)),
            "pacman did not finish: {marker}"
        );
    }
    for _ in 0..200 {
        if !Path::new(LOCK).exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!Path::new(LOCK).exists(), "no stale lock");
    for name in ["slow-a", "slow-b", "needs-virtual", "prov-b"] {
        assert_eq!(
            installed_version(&dummy(name)).as_deref(),
            Some("1.0-1"),
            "{name}"
        );
    }
    // The documented residual: the helper was gone, so the -D step did not run and the provider stays "explicit".
    assert_eq!(
        install_reason("tidewise-dummy-prov-b"),
        "Explicitly installed"
    );
    // The journal has the "accepted" entry and no "finished" one.
    let entries = journal.datagrams();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_fields(&entries[0], &[("TIDEWISE_STAGE", "accepted")]);

    // The dead helper's two files are still in the work folder, and the next job removes them.
    let leftovers = fs::read_dir(WORK_DIR).unwrap().count();
    assert_eq!(leftovers, 2, "the output files of the killed run");
    let (request, summary) = install_one("hello");
    assert_ok(&run(&summary, request.as_bytes()));
    assert_eq!(
        fs::read_dir(WORK_DIR).unwrap().count(),
        0,
        "the next job removed them"
    );
}

// ---- two helpers at once (T-12) -------------------------------------------------------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_second_helper_while_the_first_installs_is_database_locked_and_leaves_the_first_alone() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    let (first_request, first_summary) = install_one("slow-a");
    let (second_request, second_summary) = install_one("hello");
    let (mut first, first_writer) = spawn_job(&first_summary, &first_request, false);
    assert!(
        wait_for_file("/tmp/tidewise-slow-a-started", Duration::from_secs(30)),
        "the first install never started"
    );
    assert!(
        Path::new(LOCK).exists(),
        "pacman holds the lock while the script runs"
    );
    let first_files = fs::read_dir(WORK_DIR).unwrap().count();
    assert_eq!(first_files, 2, "the first helper's output files");

    // The second helper starts now: it sees the lock, stops, and does not touch the first one's files (its process is alive).
    let second = run(&second_summary, second_request.as_bytes());
    finished_with(&second, "database_locked");
    assert_eq!(
        fs::read_dir(WORK_DIR).unwrap().count(),
        first_files,
        "the live helper's files must stay"
    );
    assert_eq!(installed_version("tidewise-dummy-hello"), None);

    // The first one finishes normally and the database is consistent.
    let status =
        wait_up_to(&mut first, Duration::from_secs(60)).expect("the first helper did not finish");
    first_writer.join().unwrap();
    assert_eq!(status.code(), Some(0));
    assert_eq!(
        installed_version("tidewise-dummy-slow-a").as_deref(),
        Some("1.0-1")
    );
    assert!(!Path::new(LOCK).exists());
    assert_eq!(fs::read_dir(WORK_DIR).unwrap().count(), 0);
    // Four entries: accepted + finished for each.
    assert_eq!(journal.datagrams().len(), 4);
}

// ---- output that cannot be written and a journal that is down -------------------------------------------

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_full_stdout_changes_neither_the_job_nor_the_exit_code_nor_the_journal() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    let (request, summary) = install_one("hello");
    let mut child = spawn(HELPER, &[&summary], &[], full(), Stdio::piped());
    let writer = feed(&mut child, request.as_bytes());
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    // A panic would be exit code 101.
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        installed_version("tidewise-dummy-hello").as_deref(),
        Some("1.0-1")
    );
    assert_eq!(journal.datagrams().len(), 2);
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn without_a_journal_the_job_still_runs_and_the_result_says_the_entry_could_not_be_written() {
    assert_inside_the_test_container();
    reset_world();
    assert!(
        !Path::new(JOURNAL_SOCKET).exists(),
        "this test needs no journal"
    );
    let (request, summary) = install_one("hello");
    let ran = run(&summary, request.as_bytes());
    assert_eq!(ran.status.code(), Some(0), "{}", ran.stdout);
    let lines = ran.lines();
    let n = lines.len();
    assert_eq!(lines[n - 2], "{\"notice\":\"audit_entry_not_written\"}");
    assert_eq!(lines[n - 1], OK_LINE);
    assert_eq!(
        installed_version("tidewise-dummy-hello").as_deref(),
        Some("1.0-1")
    );
}
