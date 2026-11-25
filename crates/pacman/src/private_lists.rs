//! The app's own, private copy of the package lists (WBS 6.11, decision 0008): refreshed as the normal user, with no
//! password, without touching `/var/lib/pacman/sync`.
#![allow(dead_code)] // until the public entry is added, later in this branch

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, SystemTime};

const FAKEROOT: &str = "/usr/bin/fakeroot";
const PACMAN: &str = "/usr/bin/pacman";

/// Lists younger than this are not downloaded again just because a screen was opened.
const FRESH_FOR: Duration = Duration::from_secs(10 * 60);

/// Why the app wants fresh lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshWhy {
    /// A screen that shows an update list was opened.
    Opening,
    /// The helper just refused: the confirmed list no longer matched.
    AfterRefusal,
}

/// How a refresh command ended, when it did not work.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RefreshError {
    /// A `db.lck` is already in the private folder.
    Locked,
    /// Anything else, with pacman's own words.
    Failed { stderr: String },
}

/// The arguments for `/usr/bin/setpriv`. `folder` is the private database folder, already checked.
///
/// `setpriv --no-new-privs` comes first and then replaces itself with `fakeroot`: with that bit set, pacman can apply
/// its filesystem sandbox as a normal user, so no `--disable-sandbox*` flag is needed (measured, WBS 6.11). `-Sy`
/// appears only here, always with the private `--dbpath`: nothing is installed from these lists (golden rule 4).
fn refresh_args(folder: &str) -> Vec<String> {
    [
        "--no-new-privs",
        FAKEROOT,
        "--",
        PACMAN,
        "-Sy",
        "--dbpath",
        folder,
        "--logfile",
        "/dev/null",
    ]
    .map(str::to_string)
    .to_vec()
}

/// Reads how the refresh ended.
fn refresh_result(output: &Output) -> Result<(), RefreshError> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.contains("(unable to lock database)") {
        return Err(RefreshError::Locked);
    }
    Err(RefreshError::Failed { stderr })
}

/// Are new lists wanted? `refreshed` is when the last successful refresh ended, `asked` when this request began.
fn needs_refresh(refreshed: Option<SystemTime>, asked: SystemTime, why: RefreshWhy) -> bool {
    let Some(refreshed) = refreshed else {
        return true;
    };
    match asked.duration_since(refreshed) {
        // Refreshed before we asked.
        Ok(age) => why == RefreshWhy::AfterRefusal || age > FRESH_FOR,
        // Refreshed after we asked: another window did it while we waited. A time far ahead means the clock moved.
        Err(ahead) => ahead.duration() > FRESH_FOR,
    }
}

/// What the app should read after asking for fresh lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lists {
    /// The private copy: pass this folder as `--dbpath`.
    Private(PathBuf),
    /// The real lists, as old as they are.
    Real(NotFresh),
}

/// Why there is no fresh private copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotFresh {
    /// This session can't have one (no usable `XDG_RUNTIME_DIR`, no `fakeroot`). Not an error.
    Unavailable,
    /// Another refresh still holds the folder.
    Busy,
    /// pacman could not refresh them.
    Failed,
}

/// The private database folder, checked: `<XDG_RUNTIME_DIR>/tidewise/lists`.
#[derive(Debug)]
struct PrivateFolder {
    path: PathBuf,
}

/// An exclusive `flock` on the private folder. Having one is the proof that no refresh of ours is running.
#[derive(Debug)]
struct FolderLock<'a> {
    folder: &'a PrivateFolder,
    file: File,
}

impl PrivateFolder {
    fn prepare(runtime_dir: &Path, _local_db: &Path, _uid: u32) -> io::Result<Self> {
        Ok(Self {
            path: runtime_dir.to_path_buf(),
        })
    }

    fn dbpath(&self) -> String {
        String::new()
    }

    fn lock(&self, _wait: Duration) -> io::Result<Option<FolderLock<'_>>> {
        Ok(Some(FolderLock {
            folder: self,
            file: File::open(&self.path)?,
        }))
    }
}

impl FolderLock<'_> {
    fn remove_stale_lock(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }
}

/// One refresh: lock the folder, run pacman if the lists are not fresh enough. `run` gets the arguments for `setpriv`
/// and the locked folder, which becomes the child's stdin.
fn refresh_in(
    _folder: &PrivateFolder,
    _why: RefreshWhy,
    _wait: Duration,
    _run: impl FnOnce(&[String], File) -> io::Result<Output>,
) -> Lists {
    Lists::Real(NotFresh::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::cell::Cell;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const OK: &str = include_str!("../../../tests/fixtures/pacman/refresh-ok.txt");
    const LOCKED: &str = include_str!("../../../tests/fixtures/pacman/refresh-locked.stderr.txt");
    const DOWNLOAD: &str =
        include_str!("../../../tests/fixtures/pacman/refresh-download-failed.stderr.txt");
    const LANDLOCK: &str =
        include_str!("../../../tests/fixtures/pacman/refresh-landlock.stderr.txt");
    const FOLDER: &str = "/run/user/1000/tidewise/lists/";

    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[test]
    fn the_refresh_command_is_exactly_this() {
        assert_eq!(
            refresh_args(FOLDER),
            [
                "--no-new-privs",
                "/usr/bin/fakeroot",
                "--",
                "/usr/bin/pacman",
                "-Sy",
                "--dbpath",
                FOLDER,
                "--logfile",
                "/dev/null",
            ]
        );
    }

    #[test]
    fn setpriv_no_new_privs_always_comes_first() {
        // Without the bit pacman cannot apply its filesystem sandbox as a normal user (measured, 6.11).
        for folder in [FOLDER, "/tmp/x/", "/a b/c/"] {
            let args = refresh_args(folder);
            assert_eq!(args.first().map(String::as_str), Some("--no-new-privs"));
            assert_eq!(args.get(1).map(String::as_str), Some("/usr/bin/fakeroot"));
        }
    }

    #[test]
    fn dash_sy_only_ever_with_the_private_folder_and_nothing_that_installs_or_weakens() {
        let args = refresh_args(FOLDER);
        assert_eq!(args.iter().filter(|a| *a == "-Sy").count(), 1);
        let at = args.iter().position(|a| a == "--dbpath").expect("--dbpath");
        assert_eq!(args.get(at + 1).map(String::as_str), Some(FOLDER));
        for arg in &args {
            assert!(!arg.starts_with("--disable-sandbox"), "{arg}");
            assert!(
                ![
                    "-u",
                    "-Su",
                    "-Syu",
                    "-Syy",
                    "--noconfirm",
                    "--overwrite",
                    "-d",
                    "-dd"
                ]
                .contains(&arg.as_str())
            );
        }
    }

    #[test]
    fn a_refresh_that_worked() {
        assert_eq!(refresh_result(&output(0, OK, "")), Ok(()));
    }

    #[test]
    fn a_locked_folder_is_told_apart() {
        assert_eq!(
            refresh_result(&output(1, "", LOCKED)),
            Err(RefreshError::Locked)
        );
    }

    #[test]
    fn every_other_failure_keeps_pacmans_words() {
        for stderr in [DOWNLOAD, LANDLOCK, "", "\u{fffd}garbage"] {
            assert_eq!(
                refresh_result(&output(1, "", stderr)),
                Err(RefreshError::Failed {
                    stderr: stderr.trim().to_string()
                })
            );
        }
    }

    #[test]
    fn a_signal_is_a_failure_not_a_success() {
        let killed = Output {
            status: ExitStatus::from_raw(9),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert!(matches!(
            refresh_result(&killed),
            Err(RefreshError::Failed { .. })
        ));
    }

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn never_refreshed_means_refresh() {
        assert!(needs_refresh(None, at(1_000_000), RefreshWhy::Opening));
        assert!(needs_refresh(None, at(1_000_000), RefreshWhy::AfterRefusal));
    }

    #[test]
    fn opening_refreshes_only_lists_older_than_ten_minutes() {
        let now = at(1_000_000);
        let ago = |seconds: u64| Some(at(1_000_000 - seconds));
        assert!(!needs_refresh(ago(1), now, RefreshWhy::Opening));
        assert!(!needs_refresh(ago(600), now, RefreshWhy::Opening));
        assert!(needs_refresh(ago(601), now, RefreshWhy::Opening));
    }

    #[test]
    fn after_a_refusal_even_one_second_old_lists_are_refreshed() {
        assert!(needs_refresh(
            Some(at(999_999)),
            at(1_000_000),
            RefreshWhy::AfterRefusal
        ));
    }

    #[test]
    fn lists_refreshed_while_we_waited_are_used_as_they_are() {
        // Another window refreshed after we asked: "the second waits, then reads the result".
        for why in [RefreshWhy::Opening, RefreshWhy::AfterRefusal] {
            assert!(!needs_refresh(Some(at(1_000_005)), at(1_000_000), why));
        }
    }

    #[test]
    fn a_time_in_the_future_that_is_far_off_is_not_trusted() {
        // The clock was set back: do not keep old lists for hours.
        assert!(needs_refresh(
            Some(at(1_000_000 + 3_600)),
            at(1_000_000),
            RefreshWhy::Opening
        ));
    }

    // ---- the folder, the lock and the sequence ---------------------------------------------------------------

    /// A fake session: a runtime folder (0700) and a real local database.
    struct Session {
        _tmp: TempDir,
        runtime: PathBuf,
        local_db: PathBuf,
        uid: u32,
    }

    impl Session {
        fn new(name: &str) -> Self {
            let tmp = TempDir::new(name);
            let runtime = tmp.path().join("run");
            let local_db = tmp.path().join("db/local");
            fs::create_dir_all(&runtime).unwrap();
            fs::create_dir_all(&local_db).unwrap();
            set_mode(&runtime, 0o700);
            let uid = fs::metadata(&runtime).unwrap().uid();
            Self {
                _tmp: tmp,
                runtime,
                local_db,
                uid,
            }
        }

        fn prepare(&self) -> io::Result<PrivateFolder> {
            PrivateFolder::prepare(&self.runtime, &self.local_db, self.uid)
        }

        fn lists(&self) -> PathBuf {
            self.runtime.join("tidewise/lists")
        }
    }

    fn set_mode(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn mode_of(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().mode() & 0o7777
    }

    #[test]
    fn the_folder_is_made_0700_with_a_link_to_the_local_database() {
        let session = Session::new("make");
        let folder = session.prepare().unwrap();
        assert_eq!(folder.path, session.lists());
        assert_eq!(folder.dbpath(), format!("{}/", session.lists().display()));
        assert_eq!(mode_of(&session.runtime.join("tidewise")), 0o700);
        assert_eq!(mode_of(&session.lists()), 0o700);
        assert_eq!(
            fs::read_link(session.lists().join("local")).unwrap(),
            session.local_db
        );
        // And it is accepted again as it is.
        session.prepare().unwrap();
    }

    #[test]
    fn a_runtime_folder_that_is_not_ours_alone_is_not_used() {
        for mode in [0o755, 0o770, 0o701, 0o600, 0o1700] {
            let session = Session::new("mode");
            set_mode(&session.runtime, mode);
            assert!(session.prepare().is_err(), "{mode:o}");
            set_mode(&session.runtime, 0o700);
            assert!(!session.runtime.join("tidewise").exists(), "{mode:o}");
        }
        let session = Session::new("owner");
        let other = PrivateFolder::prepare(&session.runtime, &session.local_db, session.uid + 1);
        assert!(other.is_err());
        assert!(!session.runtime.join("tidewise").exists());
    }

    #[test]
    fn a_runtime_path_that_is_a_link_relative_or_missing_is_not_used() {
        let session = Session::new("link");
        let link = session.runtime.with_file_name("linked");
        symlink(&session.runtime, &link).unwrap();
        for path in [
            link.as_path(),
            Path::new("run"),
            Path::new(""),
            Path::new("/nonexistent-tidewise"),
            Path::new("/tmp/a\nb"),
        ] {
            let made = PrivateFolder::prepare(path, &session.local_db, session.uid);
            assert!(made.is_err(), "{path:?}");
        }
    }

    #[test]
    fn anything_planted_in_the_way_is_refused_and_left_as_it_is() {
        // tidewise as a file.
        let session = Session::new("file");
        fs::write(session.runtime.join("tidewise"), b"x").unwrap();
        assert!(session.prepare().is_err());
        assert_eq!(fs::read(session.runtime.join("tidewise")).unwrap(), b"x");

        // lists as a link to somewhere else.
        let session = Session::new("listslink");
        fs::create_dir(session.runtime.join("tidewise")).unwrap();
        set_mode(&session.runtime.join("tidewise"), 0o700);
        symlink(&session.local_db, session.lists()).unwrap();
        assert!(session.prepare().is_err());
        assert_eq!(fs::read_link(session.lists()).unwrap(), session.local_db);

        // lists with another mode is refused, never repaired.
        let session = Session::new("listsmode");
        session.prepare().unwrap();
        set_mode(&session.lists(), 0o755);
        assert!(session.prepare().is_err());
        assert_eq!(mode_of(&session.lists()), 0o755);

        // local pointing elsewhere.
        let session = Session::new("local");
        fs::create_dir(session.runtime.join("tidewise")).unwrap();
        fs::create_dir(session.lists()).unwrap();
        set_mode(&session.runtime.join("tidewise"), 0o700);
        set_mode(&session.lists(), 0o700);
        let local = session.lists().join("local");
        symlink("/etc", &local).unwrap();
        assert!(session.prepare().is_err());
        assert_eq!(fs::read_link(&local).unwrap(), Path::new("/etc"));

        // sync as a link.
        let session = Session::new("sync");
        session.prepare().unwrap();
        symlink(&session.local_db, session.lists().join("sync")).unwrap();
        assert!(session.prepare().is_err());
    }

    #[test]
    fn only_one_holder_of_the_folder_lock_at_a_time() {
        let session = Session::new("lock");
        let folder = session.prepare().unwrap();
        let first = folder.lock(Duration::ZERO).unwrap().expect("free");
        assert!(folder.lock(Duration::from_millis(60)).unwrap().is_none());
        drop(first);
        assert!(folder.lock(Duration::ZERO).unwrap().is_some());
    }

    #[test]
    fn a_stale_pacman_lock_in_the_private_folder_is_removed_under_our_flock() {
        let session = Session::new("stale");
        let folder = session.prepare().unwrap();
        let stale = session.lists().join("db.lck");
        fs::write(&stale, b"").unwrap();
        let held = folder.lock(Duration::ZERO).unwrap().expect("free");
        held.remove_stale_lock(&stale).unwrap();
        assert!(!stale.exists());
        // Nothing there is not an error.
        held.remove_stale_lock(&stale).unwrap();
    }

    #[test]
    fn no_other_path_is_ever_removed() {
        let session = Session::new("refuse");
        let folder = session.prepare().unwrap();
        fs::create_dir(session.lists().join("sync")).unwrap();
        let held = folder.lock(Duration::ZERO).unwrap().expect("free");
        let elsewhere = session.runtime.join("db.lck");
        let in_sync = session.lists().join("sync/db.lck");
        let other_name = session.lists().join("refreshed");
        let real_looking = session.local_db.with_file_name("db.lck");
        for path in [&elsewhere, &in_sync, &other_name, &real_looking] {
            fs::write(path, b"keep").unwrap();
        }
        let dotdot = session.lists().join("sync/../db.lck");
        fs::write(session.lists().join("db.lck"), b"keep").unwrap();
        for path in [
            elsewhere.as_path(),
            &in_sync,
            &other_name,
            &real_looking,
            &dotdot,
            Path::new("/var/lib/pacman/db.lck"),
            Path::new("/run/tidewise-helper-lists/db.lck"),
            Path::new("db.lck"),
        ] {
            assert!(held.remove_stale_lock(path).is_err(), "{path:?}");
        }
        for path in [&elsewhere, &in_sync, &other_name, &real_looking] {
            assert_eq!(fs::read(path).unwrap(), b"keep", "{path:?}");
        }
        assert!(session.lists().join("db.lck").exists());
    }

    #[test]
    fn a_db_lck_that_is_not_a_plain_file_is_left_alone() {
        let session = Session::new("odd");
        let folder = session.prepare().unwrap();
        let target = session.runtime.join("target");
        fs::write(&target, b"keep").unwrap();
        let odd = session.lists().join("db.lck");
        symlink(&target, &odd).unwrap();
        let held = folder.lock(Duration::ZERO).unwrap().expect("free");
        assert!(held.remove_stale_lock(&odd).is_err());
        assert_eq!(fs::read_link(&odd).unwrap(), target);
        assert_eq!(fs::read(&target).unwrap(), b"keep");
    }

    fn ran(code: i32, stderr: &str) -> io::Result<Output> {
        Ok(output(code, "", stderr))
    }

    /// Runs one refresh with a pretend pacman that ends with `code`, and says whether it was started.
    fn refresh(folder: &PrivateFolder, why: RefreshWhy, code: i32) -> (Lists, bool) {
        let started = Cell::new(false);
        let lists = refresh_in(folder, why, Duration::from_millis(50), |_, _| {
            started.set(true);
            ran(code, if code == 0 { "" } else { DOWNLOAD })
        });
        (lists, started.get())
    }

    fn private(folder: &PrivateFolder) -> Lists {
        Lists::Private(PathBuf::from(folder.dbpath()))
    }

    #[test]
    fn a_first_refresh_runs_pacman_with_the_private_folder_and_the_next_opening_does_not() {
        let session = Session::new("first");
        let folder = session.prepare().unwrap();
        let mut seen = Vec::new();
        let lists = refresh_in(&folder, RefreshWhy::Opening, Duration::ZERO, |args, _| {
            seen = args.to_vec();
            ran(0, "")
        });
        assert_eq!(lists, private(&folder));
        assert_eq!(seen, refresh_args(&folder.dbpath()));
        assert_eq!(
            refresh(&folder, RefreshWhy::Opening, 0),
            (private(&folder), false)
        );
        assert_eq!(
            refresh(&folder, RefreshWhy::AfterRefusal, 0),
            (private(&folder), true)
        );
    }

    #[test]
    fn a_failed_refresh_means_the_real_lists_and_the_next_opening_tries_again() {
        let session = Session::new("failed");
        let folder = session.prepare().unwrap();
        refresh(&folder, RefreshWhy::Opening, 0);
        // Even right after a good one: a failure after a refusal must not leave the old copy looking fresh.
        assert_eq!(
            refresh(&folder, RefreshWhy::AfterRefusal, 1),
            (Lists::Real(NotFresh::Failed), true)
        );
        assert_eq!(
            refresh(&folder, RefreshWhy::Opening, 0),
            (private(&folder), true)
        );
    }

    #[test]
    fn a_program_that_cannot_start_is_a_failed_refresh() {
        let session = Session::new("nostart");
        let folder = session.prepare().unwrap();
        let lists = refresh_in(&folder, RefreshWhy::Opening, Duration::ZERO, |_, _| {
            Err(io::Error::from(io::ErrorKind::NotFound))
        });
        assert_eq!(lists, Lists::Real(NotFresh::Failed));
    }

    #[test]
    fn a_left_over_pacman_lock_does_not_stop_the_next_refresh() {
        let session = Session::new("leftover");
        let folder = session.prepare().unwrap();
        let stale = session.lists().join("db.lck");
        fs::write(&stale, b"").unwrap();
        let lists = refresh_in(&folder, RefreshWhy::Opening, Duration::ZERO, |_, _| {
            assert!(!stale.exists(), "removed before pacman starts");
            ran(0, "")
        });
        assert_eq!(lists, private(&folder));
    }

    #[test]
    fn the_child_gets_the_locked_folder_so_the_lock_lasts_as_long_as_it_does() {
        let session = Session::new("handed");
        let folder = session.prepare().unwrap();
        let mut kept = None;
        refresh_in(&folder, RefreshWhy::Opening, Duration::ZERO, |_, stdin| {
            assert!(stdin.metadata().unwrap().is_dir());
            kept = Some(stdin); // as an orphaned pacman would
            ran(0, "")
        });
        assert!(folder.lock(Duration::ZERO).unwrap().is_none(), "still held");
        drop(kept);
        assert!(folder.lock(Duration::ZERO).unwrap().is_some());
    }

    #[test]
    fn a_busy_folder_gives_the_real_lists_and_starts_nothing_and_removes_nothing() {
        let session = Session::new("busy");
        let folder = session.prepare().unwrap();
        let other = File::open(session.lists()).unwrap();
        other.lock().unwrap();
        let running = session.lists().join("db.lck");
        fs::write(&running, b"").unwrap();
        assert_eq!(
            refresh(&folder, RefreshWhy::AfterRefusal, 0),
            (Lists::Real(NotFresh::Busy), false)
        );
        assert!(running.exists(), "that lock belongs to a running pacman");
    }

    #[test]
    fn the_second_window_waits_and_then_uses_what_the_first_one_got() {
        let session = Session::new("second");
        let folder = session.prepare().unwrap();
        let other = File::open(session.lists()).unwrap();
        other.lock().unwrap();
        let stamp = session.lists().join("refreshed");
        let first = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            fs::write(stamp, b"").unwrap();
            drop(other);
        });
        let started = Cell::new(false);
        let lists = refresh_in(
            &folder,
            RefreshWhy::AfterRefusal,
            Duration::from_secs(10),
            |_, _| {
                started.set(true);
                ran(0, "")
            },
        );
        first.join().unwrap();
        assert_eq!((lists, started.get()), (private(&folder), false));
    }
}
