//! The app's own, private copy of the package lists (WBS 6.11, decision 0008): refreshed as the normal user, with no
//! password, without touching `/var/lib/pacman/sync`.
#![allow(dead_code)] // until the public entry is added, later in this branch

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

#[cfg(test)]
mod tests {
    use super::*;
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
}
