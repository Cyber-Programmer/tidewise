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
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

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

/// Golden rule 10 against the real pacman (WBS 6.4a, review 5.8 R-08): a request that leaves a provider question to
/// pacman is refused, whatever the app would have sent. The confirmed list is the true one each time.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_install_that_leaves_a_provider_to_pacman_is_refused_and_installs_nothing() {
    assert_inside_the_test_container();
    for target in [
        "needs-virtual",
        "needs-vvirtual",
        "needs-one",
        "needs-chain",
    ] {
        reset_world();
        let journal = FakeJournal::start();
        let (request, summary) = install_one(target);
        let packages_before = package_list();
        let files_before = snapshot();

        let ran = run(&summary, request.as_bytes());
        finished_with(&ran, "provider_not_chosen");
        assert_eq!(package_list(), packages_before, "{target}");
        let changed = differences(&files_before, &snapshot());
        assert!(
            changed.is_empty(),
            "{target}: these paths changed: {changed:?}"
        );
        let entries = journal.datagrams();
        assert_eq!(entries.len(), 2, "{entries:?}");
        assert_fields(&entries[1], &[("TIDEWISE_OUTCOME", "provider_not_chosen")]);
    }
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_answer_that_does_not_settle_the_question_is_refused_and_installs_nothing() {
    assert_inside_the_test_container();
    // vprov-1 is too old for needs-vvirtual (pacman would add vprov-2 itself); hello provides nothing at all.
    for (target, dependency, chosen) in [
        ("needs-vvirtual", "tidewise-vvirtual", "vprov-1"),
        ("needs-virtual", "tidewise-virtual", "hello"),
    ] {
        reset_world();
        let names = [qualified(target), qualified(chosen)];
        let items = confirmed("-S", &[&names[0], &names[1]]);
        let (request, summary) = install_request(
            &[(REPO, &dummy(target))],
            &[(dependency, REPO, &dummy(chosen))],
            &items,
        );
        let packages_before = package_list();

        finished_with(&run(&summary, request.as_bytes()), "provider_not_chosen");
        assert_eq!(package_list(), packages_before, "{target} {chosen}");
    }
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_provider_named_as_a_target_counts_as_named() {
    assert_inside_the_test_container();
    reset_world();
    let names = [qualified("needs-virtual"), qualified("prov-b")];
    let items = confirmed("-S", &[&names[0], &names[1]]);
    let (request, summary) = install_request(
        &[
            (REPO, "tidewise-dummy-needs-virtual"),
            (REPO, "tidewise-dummy-prov-b"),
        ],
        &[],
        &items,
    );

    assert_ok(&run(&summary, request.as_bytes()));
    assert_eq!(installed_version("tidewise-dummy-prov-a"), None);
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
fn a_system_update_updates_the_package_through_a_private_copy_and_never_syncs_the_real_lists() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
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
    // Before 6.10b this counted exactly one real sync. Now there is none: the sync was of the private copy, which
    // `--logfile /dev/null` keeps out of pacman.log, and the only things logged are the upgrade itself.
    let new_log = &pacman_log()[log_before.len()..];
    let syncs = new_log
        .lines()
        .filter(|l| l.contains("synchronizing package lists"))
        .count();
    assert_eq!(syncs, 0, "no sync of the real lists any more:\n{new_log}");
    assert!(
        new_log.contains("starting full system upgrade"),
        "{new_log}"
    );
    assert!(!new_log.contains(" -Sy"), "{new_log}");

    // The real list is the private list, byte for byte, root-owned with mode 0644 (decision 0007, measurements 2).
    let real = format!("{REAL_SYNC}/tidewise-test.db");
    let private = format!("{PRIVATE_LISTS}sync/tidewise-test.db");
    assert_eq!(sha256(&real), sha256(&private));
    let meta = fs::metadata(&real).unwrap();
    assert_eq!(
        (meta.uid(), meta.gid(), meta.mode() & 0o7777),
        (0, 0, 0o644)
    );
    // The helper made its private folder on this first run: root-owned, exactly 0711, with the link to the real local
    // database and a 0711 sync/ (the download as `alpm` only works through folders it can walk, never list).
    let private_dir = PRIVATE_LISTS.trim_end_matches('/');
    let dir = fs::symlink_metadata(private_dir).unwrap();
    assert_eq!((dir.uid(), dir.gid(), dir.mode() & 0o7777), (0, 0, 0o711));
    assert_eq!(mode_of(&format!("{PRIVATE_LISTS}sync")), 0o711);
    assert_eq!(
        fs::read_link(format!("{PRIVATE_LISTS}local")).unwrap(),
        Path::new("/var/lib/pacman/local")
    );
    // Nothing of ours is left in the real folder, and the helper made no lock.
    let leftovers: Vec<String> = fingerprint(REAL_SYNC)
        .into_iter()
        .filter(|line| line.starts_with('.') || line.starts_with("download-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(!Path::new(LOCK).exists());
    assert!(!Path::new(&format!("{PRIVATE_LISTS}db.lck")).exists());

    let entries = journal.datagrams();
    assert_eq!(entries.len(), 2);
    assert_fields(
        &entries[1],
        &[("TIDEWISE_OUTCOME", "ok"), ("TIDEWISE_LISTS", "moved")],
    );
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
fn a_pending_replacement_is_refused_and_nothing_changes() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-old"]);
    add_newer("tidewise-dummy-new-1.0-1-any.pkg.tar.zst");
    assert_replacement_refused();
}

/// The helper is given exactly what the dry run prints now, and refuses with nothing installed or removed.
fn assert_replacement_refused() {
    let items = confirmed("-Su", &[]);
    assert!(!items.is_empty());
    let (request, summary) = update_request(&items);
    let packages_before = package_list();
    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "transaction_changed");
    assert_eq!(package_list(), packages_before);
    assert!(!Path::new(LOCK).exists());
}

/// Review S-01: an upgrade needs a name the replacer provides and a package is literally called that. The dry run
/// takes the replacer; the real run would decline it and install the literal package and what it needs.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_replacer_that_stands_in_for_a_needed_name_is_refused() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-old", "tidewise-dummy-upd-foo"]);
    add_newer("tidewise-dummy-upd-foo-2.0-1-any.pkg.tar.zst");
    add_newer("tidewise-dummy-new-foo-1.0-1-any.pkg.tar.zst");
    let listed: Vec<String> = confirmed("-Su", &[]).into_iter().map(|i| i.name).collect();
    assert!(
        !listed.contains(&"tidewise-dummy-foo".to_string()),
        "{listed:?}"
    );
    assert_replacement_refused();
    assert_eq!(installed_version("tidewise-dummy-foo"), None);
    assert_eq!(installed_version("tidewise-dummy-hello"), None);
}

/// Review S-02: an earlier repository holds the replaced package but takes no part in upgrades, so `-Qu` shows
/// nothing. The replacement is still in the dry run's list.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_replacement_that_qu_does_not_show_is_refused() {
    assert_inside_the_test_container();
    reset_world();
    first_repository_without_upgrade("tidewise-dummy-old-1.0-1-any.pkg.tar.zst");
    install_directly(&["tidewise-dummy-old"]);
    add_newer("tidewise-dummy-old-2.0-1-any.pkg.tar.zst");
    add_newer("tidewise-dummy-new-1.0-1-any.pkg.tar.zst");
    assert_eq!(pacman_stdout(&["-Qu"]), "");
    assert_replacement_refused();
    assert_eq!(
        installed_version("tidewise-dummy-old").as_deref(),
        Some("1.0-1")
    );
}

/// The replacer is installed already: pacman lists it again, and the real run would upgrade the replaced package.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_replacement_by_an_installed_package_is_refused() {
    assert_inside_the_test_container();
    reset_world();
    add_newer("tidewise-dummy-new-1.0-1-any.pkg.tar.zst");
    install_directly(&["tidewise-dummy-old", "tidewise-dummy-new"]);
    add_newer("tidewise-dummy-old-2.0-1-any.pkg.tar.zst");
    assert_replacement_refused();
    assert_eq!(installed_version("tidewise-dummy-hello"), None);
}

/// A versioned `Replaces` that does not fit the installed version is no replacement (`-T` says so).
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_versioned_replaces_that_does_not_fit_lets_the_update_through() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-old", "tidewise-dummy-upd-ver"]);
    add_newer("tidewise-dummy-upd-ver-2.0-1-any.pkg.tar.zst");
    let (request, summary) = update_request(&confirmed("-Su", &[]));
    let ran = run(&summary, request.as_bytes());
    assert_ok(&ran);
    assert!(installed_version("tidewise-dummy-ver").is_some());
    assert_eq!(
        installed_version("tidewise-dummy-old").as_deref(),
        Some("1.0-1")
    );
}

/// Review S-06: only the new version is in an `IgnoreGroup`. The update goes through, leaving it alone.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_with_a_new_version_in_an_ignored_group_goes_through() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-upd-grp", "tidewise-dummy-upd"]);
    add_newer("tidewise-dummy-upd-grp-2.0-1-any.pkg.tar.zst");
    add_newer("tidewise-dummy-upd-2.0-1-any.pkg.tar.zst");
    ignore_group("tidewise-grp");
    let (request, summary) = update_request(&confirmed("-Su", &[]));
    let ran = run(&summary, request.as_bytes());
    assert_ok(&ran);
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("2.0-1")
    );
    assert_eq!(
        installed_version("tidewise-dummy-upd-grp").as_deref(),
        Some("1.0-1")
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

// ---- the update flow and the real package lists (6.10b, decision 0007, review 5.8 R-02) -------------------

const LATER_UPD: &str = "tidewise-dummy-upd-2.0-1-any.pkg.tar.zst";

/// An installed package with a newer version in the repository (the real lists know it), and a SECOND change of the
/// repository that the real lists do not know: whoever syncs the real lists now changes them.
fn an_update_and_a_repository_the_real_lists_have_not_seen() -> Vec<Item> {
    install_directly(&["tidewise-dummy-upd"]);
    add_newer(LATER_UPD);
    let items = confirmed("-Su", &[]);
    assert_eq!(items.len(), 1);
    // pacman skips a download whose server file is not newer (to the second) than its own copy: let a second pass, or
    // a sync of the real lists would find nothing to change and the tests below could not tell it from no sync.
    std::thread::sleep(Duration::from_millis(1100));
    change_repository_without_syncing("tidewise-dummy-new-1.0-1-any.pkg.tar.zst");
    items
}

/// Golden rule 10 for a plain update (WBS 6.5, threat model 41): upd-virt 2.0 needs `tidewise-virtual`, which two
/// packages provide. The list is the one a dry run prints (pacman takes the first provider), but nobody named it.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_that_would_pick_a_provider_is_refused_with_the_real_lists_untouched() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-upd-virt"]);
    std::thread::sleep(Duration::from_millis(1100));
    change_repository_without_syncing("tidewise-dummy-upd-virt-2.0-1-any.pkg.tar.zst");
    let items: Vec<Item> = [("prov-a", "1.0-1"), ("upd-virt", "2.0-1")]
        .iter()
        .map(|(name, version)| Item {
            repo: REPO.to_string(),
            name: format!("tidewise-dummy-{name}"),
            version: (*version).to_string(),
        })
        .collect();
    let (request, summary) = update_request(&items);
    let lists_before = fingerprint(REAL_SYNC);
    let packages_before = package_list();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "provider_not_chosen");
    assert_eq!(fingerprint(REAL_SYNC), lists_before);
    assert_eq!(package_list(), packages_before);
    assert!(!Path::new(LOCK).exists());
}

/// A new package that is a plain dependency by its own name is no pick: the check reads the private `-Si`, and the
/// update goes on.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_that_brings_a_plain_new_dependency_passes_the_provider_check() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-needs-upd"]);
    add_newer("tidewise-dummy-upd-2.0-1-any.pkg.tar.zst");
    add_newer("tidewise-dummy-needs-upd-2.0-1-any.pkg.tar.zst");
    let items = confirmed("-Su", &[]);
    assert_eq!(items.len(), 2);
    let (request, summary) = update_request(&items);

    let ran = run(&summary, request.as_bytes());
    assert_ok(&ran);
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("2.0-1")
    );
}

/// Sets up an installed system and a newer repository, and runs the helper on what the dry run lists.
fn update_of(installed: &[&str], later: &[&str]) -> Ran {
    assert_inside_the_test_container();
    reset_world();
    install_directly(installed);
    for name in later {
        add_newer(&format!("tidewise-dummy-{name}-2.0-1-any.pkg.tar.zst"));
    }
    let (request, summary) = update_request(&confirmed("-Su", &[]));
    run(&summary, request.as_bytes())
}

/// Decision 0009: one package can provide the name, so pacman has nothing to ask.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_with_a_lone_provider_goes_through() {
    let ran = update_of(&["tidewise-dummy-upd-single"], &["upd-single"]);
    assert_ok(&ran);
    assert!(installed_version("tidewise-dummy-one-prov").is_some());
}

/// Decision 0009: an installed package already provides the name, although another listed one provides it too.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_whose_need_an_installed_package_meets_goes_through() {
    let ran = update_of(
        &["tidewise-dummy-upd-both", "tidewise-dummy-moving-b"],
        &["upd-both"],
    );
    assert_ok(&ran);
    assert!(installed_version("tidewise-dummy-moving-a").is_some());
    assert!(installed_version("tidewise-dummy-moving-b").is_some());
}

/// The installed provider's new version drops the name: pacman would ask, so the helper refuses.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_whose_installed_provider_drops_the_name_is_refused() {
    let ran = update_of(
        &["tidewise-dummy-upd-drop", "tidewise-dummy-upd-moving"],
        &["upd-drop", "upd-moving"],
    );
    finished_with(&ran, "provider_not_chosen");
    assert_eq!(
        installed_version("tidewise-dummy-upd-drop").as_deref(),
        Some("1.0-1")
    );
}

/// Review 6.5 R-01 with the real helper: the list the dry run prints would let `--ask 66` upgrade the old package and
/// install what it needs, neither of them on it. Refused, nothing changed.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_that_would_upgrade_a_replaced_package_nobody_listed_is_refused() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-old"]);
    add_newer("tidewise-dummy-old-2.0-1-any.pkg.tar.zst");
    add_newer("tidewise-dummy-new-1.0-1-any.pkg.tar.zst");
    let items = confirmed("-Su", &[]);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "tidewise-dummy-new");
    let (request, summary) = update_request(&items);
    let packages_before = package_list();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "transaction_changed");
    assert_eq!(package_list(), packages_before);
    assert!(!Path::new(LOCK).exists());
}

/// An install of `target`, with the list its dry run prints and no provider named.
fn install_of(target: &str) -> Ran {
    assert_inside_the_test_container();
    reset_world();
    let items = confirmed("-S", &[&format!("{REPO}/{target}")]);
    let (request, summary) = install_request(&[(REPO, target)], &[], &items);
    run(&summary, request.as_bytes())
}

/// Review 6.5 R-02: two packages provide `libtidewise.so=1-64` and pacman would take the first by itself.
#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_install_where_pacman_would_choose_a_shared_library_is_refused() {
    let ran = install_of("tidewise-dummy-needs-so");
    finished_with(&ran, "provider_not_chosen");
    assert_eq!(installed_version("tidewise-dummy-needs-so"), None);
    assert!(
        ran.stdout.contains("\"phase\":\"checking\""),
        "{}",
        ran.stdout
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_install_with_a_lone_shared_library_provider_goes_through() {
    let ran = install_of("tidewise-dummy-needs-oneso");
    assert_ok(&ran);
    assert!(installed_version("tidewise-dummy-so-one").is_some());
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_where_pacman_would_choose_a_shared_library_is_refused() {
    let ran = update_of(&["tidewise-dummy-upd-so"], &["upd-so"]);
    finished_with(&ran, "provider_not_chosen");
    assert_eq!(
        installed_version("tidewise-dummy-upd-so").as_deref(),
        Some("1.0-1")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn an_update_with_a_lone_shared_library_provider_goes_through() {
    let ran = update_of(&["tidewise-dummy-upd-oneso"], &["upd-oneso"]);
    assert_ok(&ran);
    assert!(installed_version("tidewise-dummy-so-one").is_some());
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_changed_update_list_leaves_the_real_sync_databases_untouched() {
    assert_inside_the_test_container();
    reset_world();
    let mut items = an_update_and_a_repository_the_real_lists_have_not_seen();
    items[0].version = "9.9-9".to_string(); // the list the user saw is not what pacman says now
    let (request, summary) = update_request(&items);
    let lists_before = fingerprint(REAL_SYNC);
    let log_before = pacman_log();
    let packages_before = package_list();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "transaction_changed");
    assert_eq!(
        fingerprint(REAL_SYNC),
        lists_before,
        "a refused update changed the real package lists"
    );
    assert_eq!(
        pacman_log(),
        log_before,
        "a refused update wrote to pacman.log"
    );
    assert_eq!(package_list(), packages_before);
    assert!(!Path::new(LOCK).exists());
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_private_lock_left_by_a_killed_sync_refuses_with_the_restart_text_and_leaves_the_real_lists_untouched()
 {
    assert_inside_the_test_container();
    reset_world();
    let items = an_update_and_a_repository_the_real_lists_have_not_seen();
    let (request, summary) = update_request(&items);
    // What a killed private sync leaves (the test makes it itself: the helper never creates or removes a lock).
    private_folder(PRIVATE_LISTS.trim_end_matches('/'), 0o711);
    let private_lock = format!("{PRIVATE_LISTS}db.lck");
    fs::write(&private_lock, b"").unwrap();
    let lock_before = fs::metadata(&private_lock).unwrap();
    let lists_before = fingerprint(REAL_SYNC);
    let log_before = pacman_log();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "pacman_failed");
    assert!(ran.stdout.contains("restart"), "{}", ran.stdout);
    // The look is only a look: the lock is the same file, nothing was created next to it, pacman never ran.
    let lock_after = fs::metadata(&private_lock).unwrap();
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
    let left_in_private: Vec<String> = fingerprint(&format!("{PRIVATE_LISTS}sync"));
    assert!(left_in_private.is_empty(), "{left_in_private:?}");
    assert_eq!(fingerprint(REAL_SYNC), lists_before);
    assert_eq!(&pacman_log()[log_before.len()..], "");
    assert!(!Path::new(LOCK).exists(), "the real db.lck appeared");
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("1.0-1")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_planted_private_folder_is_refused_and_not_repaired() {
    assert_inside_the_test_container();
    reset_world();
    let items = an_update_and_a_repository_the_real_lists_have_not_seen();
    let (request, summary) = update_request(&items);
    // Anyone who could create this folder before the helper did (it is under /run, which is root's) would have to be
    // root; the helper still does not take its word for it: another mode is refused, and left as it is.
    let dir = PRIVATE_LISTS.trim_end_matches('/');
    private_folder(dir, 0o755);
    let lists_before = fingerprint(REAL_SYNC);
    let log_before = pacman_log();

    let ran = run(&summary, request.as_bytes());
    finished_with(&ran, "pacman_failed");
    assert!(ran.stdout.contains("private folder"), "{}", ran.stdout);
    assert_eq!(mode_of(dir), 0o755, "the helper repaired the folder");
    assert_eq!(fingerprint(REAL_SYNC), lists_before);
    assert_eq!(&pacman_log()[log_before.len()..], "");
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("1.0-1")
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_repository_that_stopped_signing_is_refused_before_anything_is_written() {
    assert_inside_the_test_container();
    reset_world();
    let items = an_update_and_a_repository_the_real_lists_have_not_seen();
    let (request, summary) = update_request(&items);
    // The real folder holds a signature for the list (it was signed once); the repository now sends none. This
    // container's repository has `SigLevel = Never`, so pacman itself ignores the file: only the helper's rule acts.
    let real_sig = format!("{REAL_SYNC}/tidewise-test.db.sig");
    fs::write(&real_sig, b"an old signature").unwrap();
    let lists_before = fingerprint(REAL_SYNC);
    let log_before = pacman_log();

    let ran = run(&summary, request.as_bytes());
    let _ = fs::remove_file(&real_sig); // the test container's hygiene: the helper never does this
    finished_with(&ran, "pacman_failed");
    assert!(ran.stdout.contains("stopped signing"), "{}", ran.stdout);
    assert!(ran.stdout.contains("pacman -Syy"), "{}", ran.stdout);
    // Nothing was staged, renamed or changed in the real folder, and nothing was installed.
    let mut expected = lists_before.clone();
    expected.retain(|line| !line.starts_with("tidewise-test.db.sig "));
    let mut now = fingerprint(REAL_SYNC);
    now.retain(|line| !line.starts_with("tidewise-test.db.sig "));
    assert_eq!(now, expected, "the real folder changed");
    assert_eq!(&pacman_log()[log_before.len()..], "");
    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("1.0-1")
    );
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

// ---- what the caller controls: its stdout and its stdin (review 5.8 R-04, question 34, WBS 6.10a) --------------
//
// The helper's stdout and stdin belong to its caller. These tests are the caller that misbehaves: a stdout pipe that is
// full and never read, a stdin that stays open and silent, a request that arrives slowly and in pieces.

/// The helper's `STDIN_DEADLINE` and `FINAL_WRITE_DEADLINE` (`crates/helper/src/run.rs`; this crate has no
/// dependencies, so the numbers are repeated here and the unit test `the_bounds_are_pinned` pins the originals).
const STDIN_DEADLINE: Duration = Duration::from_secs(10);
const FINAL_WRITE_DEADLINE: Duration = Duration::from_secs(5);
/// What a loaded machine may add to a wait that is exact on paper.
const MARGIN: Duration = Duration::from_secs(5);

/// A pipe for the helper's stdout that is **full** and that nobody reads. Returns the read end, which the test must keep
/// (a reader that is closed is an error at once, which is a different case: `a_full_stdout_changes_neither…`), and the
/// write end to give to the helper. A thread writes 128 KiB into the 64 KiB pipe and then waits for ever, so every
/// write the helper makes blocks.
fn stdout_that_nobody_reads() -> (std::io::PipeReader, Stdio) {
    let (reader, writer) = std::io::pipe().unwrap();
    let mut filler = writer.try_clone().unwrap();
    std::thread::spawn(move || {
        let _ = filler.write_all(&[b'.'; 128 * 1024]);
    });
    // Time for the filler to fill the pipe.
    std::thread::sleep(Duration::from_millis(300));
    (reader, Stdio::from(writer))
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_caller_that_never_reads_stdout_does_not_stop_the_job() {
    assert_inside_the_test_container();
    reset_world();
    install_directly(&["tidewise-dummy-upd"]);
    add_newer("tidewise-dummy-upd-2.0-1-any.pkg.tar.zst");
    let items = confirmed("-Su", &[]);
    let (request, summary) = update_request(&items);
    let journal = FakeJournal::start();
    let log_before = pacman_log();

    let (_reader, stdout) = stdout_that_nobody_reads();
    let mut child = spawn(HELPER, &[&summary], &[], stdout, Stdio::piped());
    let writer = feed(&mut child, request.as_bytes());

    // An update: its first progress line is written inside `-Sy`. Before the fix that write blocked for ever, the sync
    // was done and the upgrade never ran (the lists newer than the packages: golden rule 4). Now the whole job runs
    // although the pipe is full, and the finished journal entry is written.
    let mut entries: Vec<String> = Vec::new();
    let began = Instant::now();
    let mut accepted_at = None;
    while entries.len() < 2 {
        let fresh = journal.datagrams();
        if !fresh.is_empty() && accepted_at.is_none() {
            accepted_at = Some(Instant::now());
        }
        entries.extend(fresh);
        assert!(
            began.elapsed() < Duration::from_secs(120),
            "the job did not reach its end while its stdout was full: {entries:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let job_ended = Instant::now();
    // The job did not wait for the reader (review 6.10a A-01): a job that takes the whole 5 s budget between its two
    // journal entries is being held by something. What this cannot show: a progress line that waits for room. That only
    // waits when the queue is FULL, which needs more than 1024 events, and a real update prints a handful; the unit test
    // `a_full_queue_still_delivers_the_final_outcome_when_the_reader_resumes_within_the_deadline` (3003 events, the job
    // must end in under 2 s) is the proof for that, and it was mutation-checked.
    let job_took = job_ended.duration_since(accepted_at.unwrap());
    assert!(
        job_took < FINAL_WRITE_DEADLINE,
        "the job took {job_took:?} between its journal entries: it was held by a reader that was not reading"
    );

    // The process exits too. Its writer thread is stuck in write(2) holding std's stdout lock, and that does not hold
    // the exit back: `std::rt::cleanup` calls `std::io::cleanup`, which takes that lock with `try_lock()` (stdio.rs:
    // "someone might have leaked a StdoutLock, which would otherwise cause a deadlock here"), then `exit()`.
    // The helper waits at most FINAL_WRITE_DEADLINE for the writer, once, for the notices and the final line.
    let status = wait_up_to(&mut child, FINAL_WRITE_DEADLINE + MARGIN).unwrap_or_else(|| {
        panic!(
            "the helper had not exited {FINAL_WRITE_DEADLINE:?} + {MARGIN:?} after its job ended"
        )
    });
    let after_job = job_ended.elapsed();
    writer.join().unwrap();
    assert_eq!(
        status.code(),
        Some(0),
        "the exit code is the job's, not the reader's"
    );
    assert!(
        after_job < FINAL_WRITE_DEADLINE + MARGIN,
        "exit took {after_job:?} after the job"
    );

    assert_eq!(
        installed_version("tidewise-dummy-upd").as_deref(),
        Some("2.0-1")
    );
    assert!(!Path::new(LOCK).exists());
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert_fields(
        &entries[1],
        &[("TIDEWISE_OUTCOME", "ok"), ("TIDEWISE_LISTS", "moved")],
    );
    // The sync was of the private copy (its progress lines are the ones that filled the pipe), so pacman.log has the
    // upgrade and no sync of the real lists.
    let new_log = &pacman_log()[log_before.len()..];
    let syncs = new_log
        .lines()
        .filter(|l| l.contains("synchronizing package lists"))
        .count();
    assert_eq!(
        syncs, 0,
        "no sync of the real lists, then the upgrade:\n{new_log}"
    );
    assert!(
        new_log.contains("starting full system upgrade"),
        "{new_log}"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_silent_stdin_gives_up_after_the_deadline() {
    assert_inside_the_test_container();
    reset_world();
    let journal = FakeJournal::start();
    let log_before = pacman_log();

    let mut child = spawn(
        HELPER,
        &["update system (1 package)"],
        &[],
        Stdio::piped(),
        Stdio::piped(),
    );
    // stdin stays open and nothing is ever written to it.
    let _held_open = child.stdin.take().unwrap();
    let began = Instant::now();
    let status = wait_up_to(&mut child, STDIN_DEADLINE + MARGIN)
        .expect("the helper never gave up on a stdin that stays silent");
    let waited = began.elapsed();

    assert!(
        waited >= STDIN_DEADLINE - Duration::from_secs(1),
        "it refused after {waited:?}, before the deadline: a slow caller would be punished"
    );
    assert_eq!(status.code(), Some(1));
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    assert_eq!(stdout, "{\"finished\":{\"error\":\"malformed\"}}\n");
    let entries = journal.datagrams();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_fields(&entries[0], &[("TIDEWISE_OUTCOME", "malformed")]);
    assert_eq!(
        &pacman_log()[log_before.len()..],
        "",
        "no pacman was started"
    );
}

#[test]
#[ignore = "runs real pacman as root: only inside the test container (tests/container/run.sh)"]
fn a_request_larger_than_the_pipe_buffer_written_in_chunks_is_accepted_within_the_deadline() {
    assert_inside_the_test_container();
    reset_world();
    let (request, summary) = install_one("hello");
    // 192 KiB of white space after the opening brace is still one valid JSON request, three times the 64 KiB a pipe
    // holds: the app has to write it while the helper reads, and a request that arrives slowly is not a silent one.
    let padded = format!("{{{}{}", " ".repeat(192 * 1024), &request[1..]);
    assert!(padded.len() > 64 * 1024 && padded.len() < MAX_REQUEST_BYTES);

    let mut child = spawn(HELPER, &[&summary], &[], Stdio::piped(), Stdio::piped());
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        let began = Instant::now();
        for chunk in padded.as_bytes().chunks(16 * 1024) {
            stdin.write_all(chunk).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        }
        // `stdin` is dropped here: that closes it, which is how the helper knows the request is complete.
        began.elapsed()
    });
    let output = child.wait_with_output().unwrap();
    let writing_took = writer.join().unwrap();

    assert!(
        writing_took > Duration::from_millis(1000) && writing_took < STDIN_DEADLINE,
        "writing took {writing_took:?}"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stdout.lines().last(), Some(OK_LINE), "{stdout}");
    assert_eq!(
        installed_version("tidewise-dummy-hello").as_deref(),
        Some("1.0-1")
    );
}
