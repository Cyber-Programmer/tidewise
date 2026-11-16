//! Test support: a throwaway folder for the unit tests. Compiled only for tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// A folder under the system temp dir, removed when dropped. (No `tempfile` crate.) The one TempDir of this crate's
/// unit tests, so the only place they delete a folder. The counter makes every folder unique inside a process,
/// whatever the test's name.
pub struct TempDir(PathBuf);

impl TempDir {
    #[expect(
        clippy::disallowed_methods,
        reason = "the unit tests' one TempDir removes a stale folder of its own name before it creates it"
    )]
    pub fn new(name: &str) -> Self {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let number = COUNT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "tidewise-pacman-test-{}-{number}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    #[expect(
        clippy::disallowed_methods,
        reason = "the unit tests' one TempDir removes its own folder when the test ends"
    )]
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
