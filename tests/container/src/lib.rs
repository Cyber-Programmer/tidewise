//! Helpers for the tests that run real pacman as root (WBS 5.7).
//!
//! Those tests are dangerous on a normal machine, so each of them starts with `require_container()`. It says yes only
//! inside **our** test image, never on the host and never in some other container (for example a distrobox that
//! shares your home folder).
//!
//! Library code must not panic, so clippy forbids `unwrap`, `expect`, indexing and `panic!` here (tests may use them).
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]

use std::fmt;
use std::path::Path;

/// Podman (and Docker) create this file inside every container. Relative, so it can be joined to any root folder.
pub const CONTAINER_MARKER: &str = "run/.containerenv";

/// The local test repository's database. Only our image builds it (see `tests/container/Containerfile`), so it tells
/// our image apart from any other container.
pub const TEST_REPO_DB: &str = "srv/tidewise-test/tidewise-test.db.tar.gz";

/// Why a container test refuses to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardError {
    /// There is no container marker: this looks like a normal machine.
    NotInContainer,
    /// It is a container, but not the Tidewise test image (no local test repository).
    NotOurImage,
}

impl fmt::Display for GuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuardError::NotInContainer => write!(
                f,
                "this test runs real pacman as root, so it only runs inside the Tidewise test container \
                 (this is not a container). Use tests/container/run.sh"
            ),
            GuardError::NotOurImage => write!(
                f,
                "this is a container, but not the Tidewise test image (the local test repository is missing). \
                 Use tests/container/run.sh"
            ),
        }
    }
}

impl std::error::Error for GuardError {}

/// Checks that `root` looks like the inside of our test image. `root` is `/` in real use; tests pass a temporary
/// folder so the check itself can be tested on any machine.
///
/// Both must be regular files: the container marker and the test repository's database.
pub fn require_container_at(root: &Path) -> Result<(), GuardError> {
    // `is_file()` is false for a folder and for anything that can't be read, so odd cases are refused, not crashes.
    if !root.join(CONTAINER_MARKER).is_file() {
        return Err(GuardError::NotInContainer);
    }
    if !root.join(TEST_REPO_DB).is_file() {
        return Err(GuardError::NotOurImage);
    }
    Ok(())
}

/// The check every container test starts with: are we inside our test image, right now?
pub fn require_container() -> Result<(), GuardError> {
    require_container_at(Path::new("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A throwaway folder standing in for `/`, removed when dropped. (No `tempfile` crate: this crate has no
    /// dependencies.)
    struct FakeRoot(PathBuf);

    impl FakeRoot {
        fn new(test: &str) -> FakeRoot {
            let path =
                std::env::temp_dir().join(format!("tidewise-guard-{}-{test}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            FakeRoot(path)
        }

        fn add_file(&self, relative: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"").unwrap();
        }

        fn add_dir(&self, relative: &str) {
            fs::create_dir_all(self.0.join(relative)).unwrap();
        }
    }

    impl Drop for FakeRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn an_ordinary_machine_is_refused() {
        let root = FakeRoot::new("ordinary");
        assert_eq!(
            require_container_at(&root.0),
            Err(GuardError::NotInContainer)
        );
    }

    #[test]
    fn the_container_marker_and_the_test_repo_together_are_accepted() {
        let root = FakeRoot::new("both");
        root.add_file(CONTAINER_MARKER);
        root.add_file(TEST_REPO_DB);
        assert_eq!(require_container_at(&root.0), Ok(()));
    }

    #[test]
    fn some_other_container_is_refused() {
        // Like a distrobox: it has the marker but not our test repository.
        let root = FakeRoot::new("other");
        root.add_file(CONTAINER_MARKER);
        assert_eq!(require_container_at(&root.0), Err(GuardError::NotOurImage));
    }

    #[test]
    fn the_test_repo_alone_is_refused() {
        // Without the container marker it is not a container, whatever else is lying around.
        let root = FakeRoot::new("repo-only");
        root.add_file(TEST_REPO_DB);
        assert_eq!(
            require_container_at(&root.0),
            Err(GuardError::NotInContainer)
        );
    }

    #[test]
    fn a_folder_named_like_the_marker_does_not_count() {
        let root = FakeRoot::new("marker-dir");
        root.add_dir(CONTAINER_MARKER);
        root.add_file(TEST_REPO_DB);
        assert_eq!(
            require_container_at(&root.0),
            Err(GuardError::NotInContainer)
        );
    }

    #[test]
    fn a_folder_named_like_the_database_does_not_count() {
        let root = FakeRoot::new("db-dir");
        root.add_file(CONTAINER_MARKER);
        root.add_dir(TEST_REPO_DB);
        assert_eq!(require_container_at(&root.0), Err(GuardError::NotOurImage));
    }

    #[test]
    fn a_missing_root_is_refused_not_a_crash() {
        let missing = Path::new("/nonexistent-tidewise-guard-root");
        assert_eq!(
            require_container_at(missing),
            Err(GuardError::NotInContainer)
        );
    }

    #[test]
    fn refusal_messages_are_plain_and_point_to_run_sh() {
        for error in [GuardError::NotInContainer, GuardError::NotOurImage] {
            let text = error.to_string();
            assert!(text.contains("container"), "{text}");
            assert!(text.contains("tests/container/run.sh"), "{text}");
        }
    }
}
