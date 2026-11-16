//! The helper's only way to delete a file (WBS 5.4; golden rule 7; threats T-03, T-17, T-20).
//!
//! STUB for the red commit: it always refuses, so the tests below fail on their assertions. The real function follows.

use std::path::Path;

use crate::workdir::WorkDir;

/// Why a file was not removed. A code only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalError {
    /// The path is not directly inside the work folder (a relative path or a `..` counts as outside).
    OutsideFolder,
    /// The path is not a regular file: a symlink, a directory, a socket or anything else.
    NotARegularFile,
    /// The file could not be looked at or removed.
    Io,
}

/// Removes one regular file that sits directly in the work folder.
pub fn remove_work_file(_dir: &WorkDir, _path: &Path) -> Result<(), RemovalError> {
    Err(RemovalError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, symlink};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    fn me() -> u32 {
        fs::metadata("/proc/self").unwrap().uid()
    }

    /// A temporary parent folder holding `work/` (the verified work folder) and room for files beside it.
    fn setup(test: &str) -> (TempDir, WorkDir, PathBuf) {
        let temp = TempDir::new(test);
        let work = temp.join("work");
        let dir = WorkDir::open(&work, me()).unwrap();
        (temp, dir, work)
    }

    fn exists(path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok()
    }

    #[test]
    fn a_regular_file_in_the_folder_is_removed() {
        let (_temp, dir, work) = setup("regular");
        let file = work.join("tidewise-helper-1-1.out");
        fs::write(&file, "x").unwrap();
        assert_eq!(remove_work_file(&dir, &file), Ok(()));
        assert!(!exists(&file));
    }

    #[test]
    fn a_symlink_is_refused_and_its_target_survives() {
        let (temp, dir, work) = setup("symlink");
        let target = temp.join("precious");
        fs::write(&target, "keep me").unwrap();
        let link = work.join("link");
        symlink(&target, &link).unwrap();
        assert_eq!(
            remove_work_file(&dir, &link),
            Err(RemovalError::NotARegularFile)
        );
        assert!(exists(&link), "the link itself must stay too");
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep me");
    }

    #[test]
    fn a_symlink_to_a_directory_is_refused_and_the_directory_survives() {
        let (temp, dir, work) = setup("symlink-dir");
        let target = temp.join("precious-dir");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("inside"), "keep me").unwrap();
        let link = work.join("link");
        symlink(&target, &link).unwrap();
        assert_eq!(
            remove_work_file(&dir, &link),
            Err(RemovalError::NotARegularFile)
        );
        assert!(exists(&target.join("inside")));
    }

    #[test]
    fn a_directory_in_the_folder_is_refused_and_survives() {
        let (_temp, dir, work) = setup("directory");
        let sub = work.join("sub");
        fs::create_dir(&sub).unwrap();
        assert_eq!(
            remove_work_file(&dir, &sub),
            Err(RemovalError::NotARegularFile)
        );
        assert!(sub.is_dir());
    }

    #[test]
    fn a_socket_is_not_a_regular_file() {
        let (_temp, dir, work) = setup("socket");
        let path = work.join("socket");
        let _listener = UnixListener::bind(&path).unwrap();
        assert_eq!(
            remove_work_file(&dir, &path),
            Err(RemovalError::NotARegularFile)
        );
        assert!(exists(&path));
    }

    #[test]
    fn a_file_in_another_folder_is_refused_and_survives() {
        let (temp, dir, _work) = setup("elsewhere");
        let other = temp.join("elsewhere");
        fs::create_dir(&other).unwrap();
        let file = other.join("file");
        fs::write(&file, "keep me").unwrap();
        assert_eq!(
            remove_work_file(&dir, &file),
            Err(RemovalError::OutsideFolder)
        );
        assert!(exists(&file));
    }

    #[test]
    fn a_path_outside_that_is_a_symlink_is_refused_as_outside_without_a_look() {
        let (temp, dir, _work) = setup("outside-link");
        let target = temp.join("precious");
        fs::write(&target, "keep me").unwrap();
        let link = temp.join("link");
        symlink(&target, &link).unwrap();
        assert_eq!(
            remove_work_file(&dir, &link),
            Err(RemovalError::OutsideFolder)
        );
        assert!(exists(&link) && exists(&target));
    }

    #[test]
    fn a_dotdot_path_is_refused_and_survives() {
        let (temp, dir, work) = setup("dotdot");
        let outside = temp.join("outside");
        fs::write(&outside, "keep me").unwrap();
        // work/../outside is the file beside the folder.
        assert_eq!(
            remove_work_file(&dir, &work.join("..").join("outside")),
            Err(RemovalError::OutsideFolder)
        );
        assert!(exists(&outside));
        // work/sub/../file really is a file of the folder, and is still refused: the spelling is what counts.
        fs::create_dir(work.join("sub")).unwrap();
        let file = work.join("file");
        fs::write(&file, "x").unwrap();
        assert_eq!(
            remove_work_file(&dir, &work.join("sub").join("..").join("file")),
            Err(RemovalError::OutsideFolder)
        );
        assert!(exists(&file));
    }

    #[test]
    fn a_relative_path_is_refused() {
        let (_temp, dir, _work) = setup("relative");
        assert_eq!(
            remove_work_file(&dir, Path::new("tidewise-helper-1-1.out")),
            Err(RemovalError::OutsideFolder)
        );
    }

    #[test]
    fn a_file_in_a_subfolder_of_the_work_folder_is_refused() {
        let (_temp, dir, work) = setup("subfolder");
        let sub = work.join("sub");
        fs::create_dir(&sub).unwrap();
        let file = sub.join("file");
        fs::write(&file, "x").unwrap();
        assert_eq!(
            remove_work_file(&dir, &file),
            Err(RemovalError::OutsideFolder)
        );
        assert!(exists(&file));
    }

    #[test]
    fn the_work_folder_itself_is_refused() {
        let (_temp, dir, work) = setup("itself");
        assert_eq!(
            remove_work_file(&dir, &work),
            Err(RemovalError::OutsideFolder)
        );
        assert!(work.is_dir());
    }

    #[test]
    fn a_file_that_is_not_there_is_an_io_error() {
        let (_temp, dir, work) = setup("missing");
        assert_eq!(
            remove_work_file(&dir, &work.join("nope")),
            Err(RemovalError::Io)
        );
    }
}
