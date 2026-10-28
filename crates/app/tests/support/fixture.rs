//! STUB (red phase).
use std::path::PathBuf;

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub root: PathBuf,
}

pub fn build() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("home");
    std::fs::create_dir_all(&root).unwrap();
    Fixture { dir, root }
}
