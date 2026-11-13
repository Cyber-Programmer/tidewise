//! The package lists (pacman's sync databases): do they exist, and how old are they?
//!
//! Pure reading of files and of `pacman-conf` text. The process is started in `run.rs`, and the clock stays
//! out of here: the caller compares `SyncLists::newest` with its own "now".

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::{PacmanError, ParseError};

/// What is on disk for the configured repositories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncLists {
    /// The newest modification time among the repositories' `.db` files, or `None` when none exists
    /// ("never synced": the design's "no package lists" card). pacman gives a downloaded file the mirror's
    /// timestamp, so this says how old the lists are, not when this machine last ran `-Sy`.
    pub newest: Option<SystemTime>,
    /// Repositories that have no `.db` file, in the order pacman lists them.
    pub missing: Vec<String>,
}

/// Reads the output of `LC_ALL=C pacman-conf DBPath`: one absolute path.
pub(crate) fn parse_dbpath(_text: &str) -> Result<PathBuf, ParseError> {
    Ok(PathBuf::new())
}

/// Reads the output of `LC_ALL=C pacman-conf --repo-list`: one repository name per line.
pub(crate) fn parse_repo_list(_text: &str) -> Result<Vec<String>, ParseError> {
    Ok(Vec::new())
}

/// Looks for `<sync_dir>/<repo>.db` for each repository. Nothing else in the folder is looked at (pacman
/// leaves `download-*` folders there).
///
/// A missing file, or a missing folder, is not an error: the repository goes in `missing`. Any other failure
/// to read a file's details is `PacmanError::ReadLists`.
pub(crate) fn sync_lists_in(_sync_dir: &Path, _repos: &[String]) -> Result<SyncLists, PacmanError> {
    Ok(SyncLists {
        newest: None,
        missing: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::time::Duration;

    const DBPATH: &str = include_str!("../../../tests/fixtures/pacman/pacman-conf-dbpath.txt");
    const REPO_LIST: &str =
        include_str!("../../../tests/fixtures/pacman/pacman-conf-repo-list.txt");

    // --- parse_dbpath ---

    fn bad_config(result: Result<impl std::fmt::Debug, ParseError>) -> (&'static str, usize) {
        match result {
            Err(ParseError::BadConfigLine {
                what, line_number, ..
            }) => (what, line_number),
            other => panic!("expected BadConfigLine, got {other:?}"),
        }
    }

    #[test]
    fn dbpath_fixture_is_the_absolute_path() {
        assert_eq!(
            parse_dbpath(DBPATH).unwrap(),
            PathBuf::from("/var/lib/pacman/")
        );
    }

    #[test]
    fn dbpath_final_newline_is_optional() {
        assert_eq!(
            parse_dbpath("/var/lib/pacman/").unwrap(),
            PathBuf::from("/var/lib/pacman/")
        );
    }

    #[test]
    fn dbpath_empty_output_is_an_error() {
        assert_eq!(bad_config(parse_dbpath("")), ("DBPath", 1));
        assert_eq!(bad_config(parse_dbpath("\n")), ("DBPath", 1));
    }

    #[test]
    fn dbpath_must_be_absolute() {
        assert_eq!(bad_config(parse_dbpath("var/lib/pacman/\n")), ("DBPath", 1));
        assert_eq!(bad_config(parse_dbpath("../pacman\n")), ("DBPath", 1));
    }

    #[test]
    fn dbpath_must_be_one_line() {
        assert_eq!(
            bad_config(parse_dbpath("/var/lib/pacman/\n/other/\n")),
            ("DBPath", 2)
        );
    }

    #[test]
    fn dbpath_refuses_control_characters() {
        assert_eq!(
            bad_config(parse_dbpath("/var/lib\0/pacman/\n")),
            ("DBPath", 1)
        );
        assert_eq!(
            bad_config(parse_dbpath("/var/lib/\tpacman/\n")),
            ("DBPath", 1)
        );
    }

    // --- parse_repo_list ---

    #[test]
    fn repo_list_fixture_gives_three_repos_in_order() {
        assert_eq!(
            parse_repo_list(REPO_LIST).unwrap(),
            ["core", "extra", "multilib"]
        );
    }

    #[test]
    fn repo_list_empty_output_is_no_repos() {
        assert_eq!(parse_repo_list("").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn repo_list_refuses_bad_names() {
        assert_eq!(
            bad_config(parse_repo_list("core\n\nextra\n")),
            ("repository", 2)
        );
        assert_eq!(
            bad_config(parse_repo_list("core\na/b\n")),
            ("repository", 2)
        );
        assert_eq!(bad_config(parse_repo_list("local\n")), ("repository", 1));
        assert_eq!(bad_config(parse_repo_list("co\tre\n")), ("repository", 1));
    }

    // --- sync_lists_in ---

    /// A folder under the system temp dir, removed when dropped. Unique per test name and process.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("tidewise-sync-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    /// Creates `dir/file` with the given modification time.
    fn touch(dir: &Path, file: &str, seconds: u64) {
        let file = File::create(dir.join(file)).unwrap();
        file.set_modified(at(seconds)).unwrap();
    }

    fn repos(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn a_missing_sync_folder_means_every_repo_is_missing() {
        let temp = TempDir::new("no-folder");
        let lists = sync_lists_in(&temp.path().join("sync"), &repos(&["core", "extra"])).unwrap();
        assert_eq!(lists.newest, None);
        assert_eq!(lists.missing, ["core", "extra"]);
    }

    #[test]
    fn an_empty_sync_folder_means_never_synced() {
        let temp = TempDir::new("empty");
        let lists = sync_lists_in(temp.path(), &repos(&["core", "extra"])).unwrap();
        assert_eq!(lists.newest, None);
        assert_eq!(lists.missing, ["core", "extra"]);
    }

    #[test]
    fn the_newest_file_wins() {
        let temp = TempDir::new("newest");
        touch(temp.path(), "core.db", 1_000);
        touch(temp.path(), "extra.db", 3_000);
        touch(temp.path(), "multilib.db", 2_000);
        let lists = sync_lists_in(temp.path(), &repos(&["core", "extra", "multilib"])).unwrap();
        assert_eq!(lists.newest, Some(at(3_000)));
        assert!(lists.missing.is_empty());
    }

    #[test]
    fn missing_repos_are_listed_in_the_order_given_and_the_rest_still_count() {
        let temp = TempDir::new("some-missing");
        touch(temp.path(), "core.db", 1_000);
        let lists = sync_lists_in(temp.path(), &repos(&["core", "extra", "multilib"])).unwrap();
        assert_eq!(lists.newest, Some(at(1_000)));
        assert_eq!(lists.missing, ["extra", "multilib"]);
    }

    #[test]
    fn download_folders_and_unlisted_files_are_ignored() {
        let temp = TempDir::new("others");
        touch(temp.path(), "core.db", 1_000);
        touch(temp.path(), "other.db", 9_000);
        touch(temp.path(), "core.db.part", 9_000);
        fs::create_dir(temp.path().join("download-f5xkps")).unwrap();
        let lists = sync_lists_in(temp.path(), &repos(&["core"])).unwrap();
        assert_eq!(lists.newest, Some(at(1_000)));
        assert!(lists.missing.is_empty());
    }

    #[test]
    fn a_folder_named_like_a_list_is_not_a_list() {
        let temp = TempDir::new("dir-named-db");
        fs::create_dir(temp.path().join("core.db")).unwrap();
        let lists = sync_lists_in(temp.path(), &repos(&["core"])).unwrap();
        assert_eq!(lists.newest, None);
        assert_eq!(lists.missing, ["core"]);
    }

    #[test]
    fn no_repositories_means_nothing_found() {
        let temp = TempDir::new("no-repos");
        touch(temp.path(), "core.db", 1_000);
        let lists = sync_lists_in(temp.path(), &[]).unwrap();
        assert_eq!(lists.newest, None);
        assert!(lists.missing.is_empty());
    }

    #[test]
    fn a_failure_other_than_not_found_is_a_read_error_with_the_path() {
        // The "sync folder" is a plain file, so looking inside it fails with "not a directory".
        let temp = TempDir::new("not-a-dir");
        touch(temp.path(), "sync", 1_000);
        let sync_file = temp.path().join("sync");
        let error = sync_lists_in(&sync_file, &repos(&["core"])).unwrap_err();
        assert!(
            matches!(&error, PacmanError::ReadLists { path, .. }
                if *path == sync_file.join("core.db")),
            "got {error:?}"
        );
    }
}
