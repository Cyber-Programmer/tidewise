//! The helper's work folder, `/run/tidewise-helper/` (threats T-03, T-17, T-20; human's addition to 5.3b).
//!
//! pacman's stdout and stderr go to files here, not to pipes. Measured in the test container: when the reader of a
//! pipe goes away, pacman is killed by SIGPIPE at its next write and leaves a stale `db.lck`. A file cannot go away.
//!
//! The folder must be exactly what we expect before anything is put in it: created if it is missing (mode 0700), and
//! if it exists it must be a real directory (not a symlink, not a file), owned by root, with mode 0700. Anything else
//! is refused, never repaired. `/run` is root-owned, so only root can have made or changed it. Files are created with
//! `create_new` (`O_CREAT|O_EXCL`: an existing path, symlinks included, is refused), mode 0600, with names the helper
//! builds from its own process id and a counter: no caller-chosen name.
//!
//! At the start of a job, leftovers of an earlier helper that was killed are removed, and only those: regular files
//! named exactly `tidewise-helper-<pid>-<n>.out` or `.err`, and only when no process with that id is alive (another
//! helper running at the same time keeps its files). Nothing else in the folder is touched.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// The folder the real helper uses.
pub const WORK_DIR: &str = "/run/tidewise-helper";

/// Why the folder cannot be used. A code only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkDirError {
    /// The path is a symlink, a file or anything but a real directory.
    NotADirectory,
    /// The directory belongs to someone else.
    WrongOwner,
    /// The directory's mode is not exactly 0700.
    WrongMode,
    /// The folder could not be created or read.
    Io,
}

/// A verified work folder: in production, only ever `/run/tidewise-helper` (`WorkDir::open_default`). A constructor for
/// any other folder exists only in the test builds, so outside them no other folder can be made into a `WorkDir`:
///
/// ```compile_fail
/// use tidewise_helper::workdir::WorkDir;
/// let _ = WorkDir::open(std::path::Path::new("/tmp/anywhere"), 0);
/// ```
///
/// and the one way to get one compiles:
///
/// ```no_run
/// use tidewise_helper::workdir::WorkDir;
/// let _dir = WorkDir::open_default();
/// ```
#[derive(Debug)]
pub struct WorkDir {
    path: PathBuf,
}

/// The two files of one pacman run, created and opened for writing.
#[derive(Debug)]
pub struct OutputFiles {
    pub out_path: PathBuf,
    pub err_path: PathBuf,
    pub out: File,
    pub err: File,
}

/// True if a process with this id exists (`/proc/<pid>`). A recycled id counts as alive: the cleanup then keeps a file it
/// could have removed, which is the safe way round.
pub fn process_is_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// The test the helper uses for leftovers: a process other than this one is alive. A file with our own pid cannot be a
/// live helper's (we have not made any yet), so it is a leftover of an earlier helper whose pid was reused (review R-08).
pub fn another_process_is_alive(pid: u32) -> bool {
    pid != std::process::id() && process_is_alive(pid)
}

/// `tidewise-helper-<pid>-<serial>.<extension>`
pub fn file_name(pid: u32, serial: u32, extension: &str) -> String {
    format!("tidewise-helper-{pid}-{serial}.{extension}")
}

impl WorkDir {
    /// Creates `path` (mode 0700) if it is missing, then verifies it: a real directory, owned by `owner_uid`, mode 0700.
    pub fn open(path: &Path, owner_uid: u32) -> Result<WorkDir, WorkDirError> {
        match fs::symlink_metadata(path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match DirBuilder::new().mode(0o700).create(path) {
                    Ok(()) => {}
                    // Someone made it between our look and our create: verify what is there below.
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(WorkDirError::Io),
                }
            }
            Err(_) => return Err(WorkDirError::Io),
        }

        // symlink_metadata does not follow a symlink, so a symlink is not a directory here.
        let meta = fs::symlink_metadata(path).map_err(|_| WorkDirError::Io)?;
        if !meta.file_type().is_dir() {
            return Err(WorkDirError::NotADirectory);
        }
        if meta.uid() != owner_uid {
            return Err(WorkDirError::WrongOwner);
        }
        if meta.mode() & 0o7777 != 0o700 {
            return Err(WorkDirError::WrongMode);
        }
        Ok(WorkDir {
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Removes leftover files of the helper's own name pattern whose process is gone. Returns how many.
    pub fn remove_leftovers(&self, process_is_alive: &dyn Fn(u32) -> bool) -> u32 {
        let Ok(entries) = fs::read_dir(&self.path) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(pid_of_our_file) else {
                continue;
            };
            // DirEntry::file_type does not follow symlinks: only a regular file is ever removed.
            let is_regular_file = entry.file_type().is_ok_and(|kind| kind.is_file());
            if is_regular_file && !process_is_alive(pid) && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    /// Creates both output files of a run (mode 0600, refused if either name already exists).
    pub fn create_output_files(&self, pid: u32, serial: u32) -> Result<OutputFiles, WorkDirError> {
        let out_path = self.path.join(file_name(pid, serial, "out"));
        let err_path = self.path.join(file_name(pid, serial, "err"));
        let out = create_new(&out_path)?;
        let err = match create_new(&err_path) {
            Ok(file) => file,
            Err(error) => {
                // Never leave half a pair behind.
                let _ = fs::remove_file(&out_path);
                return Err(error);
            }
        };
        Ok(OutputFiles {
            out_path,
            err_path,
            out,
            err,
        })
    }
}

/// `O_CREAT|O_EXCL`, mode 0600: an existing path (a symlink too) is refused, never opened or followed.
fn create_new(path: &Path) -> Result<File, WorkDirError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| WorkDirError::Io)
}

/// The process id in `tidewise-helper-<pid>-<n>.out|.err`, and only for exactly that shape: digits only, at most ten.
fn pid_of_our_file(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("tidewise-helper-")?;
    let rest = rest
        .strip_suffix(".out")
        .or_else(|| rest.strip_suffix(".err"))?;
    let (pid, serial) = rest.split_once('-')?;
    let is_number = |text: &str| {
        !text.is_empty() && text.len() <= 10 && text.bytes().all(|byte| byte.is_ascii_digit())
    };
    if is_number(pid) && is_number(serial) {
        pid.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    /// A throwaway folder under the temp dir, removed when dropped. (No `tempfile` crate.)
    struct Temp(PathBuf);

    impl Temp {
        fn new(test: &str) -> Temp {
            let path = std::env::temp_dir()
                .join(format!("tidewise-workdir-{}-{test}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Temp(path)
        }
        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn me() -> u32 {
        fs::metadata("/proc/self").unwrap().uid()
    }

    fn mode_of(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().mode() & 0o7777
    }

    fn make_dir(path: &Path, mode: u32) {
        fs::create_dir(path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    // ---- opening ------------------------------------------------------------------------------------------

    #[test]
    fn a_missing_folder_is_created_with_mode_0700_and_then_verified() {
        let temp = Temp::new("create");
        let path = temp.join("work");
        let dir = WorkDir::open(&path, me()).unwrap();
        assert_eq!(dir.path(), path);
        assert!(path.is_dir());
        assert_eq!(mode_of(&path), 0o700);
    }

    #[test]
    fn an_existing_folder_that_is_right_is_used_as_it_is() {
        let temp = Temp::new("existing");
        let path = temp.join("work");
        make_dir(&path, 0o700);
        fs::write(path.join("keep-me.txt"), b"x").unwrap();
        assert!(WorkDir::open(&path, me()).is_ok());
        assert!(path.join("keep-me.txt").exists());
    }

    #[test]
    fn a_symlink_to_a_good_folder_is_refused() {
        let temp = Temp::new("symlink");
        let real = temp.join("real");
        make_dir(&real, 0o700);
        let link = temp.join("work");
        symlink(&real, &link).unwrap();
        assert_eq!(
            WorkDir::open(&link, me()).unwrap_err(),
            WorkDirError::NotADirectory
        );
    }

    #[test]
    fn a_dangling_symlink_is_refused_not_followed_or_created_through() {
        let temp = Temp::new("dangling");
        let link = temp.join("work");
        symlink(temp.join("nowhere"), &link).unwrap();
        assert_eq!(
            WorkDir::open(&link, me()).unwrap_err(),
            WorkDirError::NotADirectory
        );
        assert!(!temp.join("nowhere").exists());
    }

    #[test]
    fn a_file_where_the_folder_should_be_is_refused() {
        let temp = Temp::new("file");
        let path = temp.join("work");
        fs::write(&path, b"x").unwrap();
        assert_eq!(
            WorkDir::open(&path, me()).unwrap_err(),
            WorkDirError::NotADirectory
        );
    }

    #[test]
    fn any_mode_but_0700_is_refused_and_not_repaired() {
        for mode in [0o755, 0o750, 0o777, 0o701, 0o600, 0o1700, 0o2700] {
            let temp = Temp::new(&format!("mode-{mode:o}"));
            let path = temp.join("work");
            make_dir(&path, mode);
            assert_eq!(
                WorkDir::open(&path, me()).unwrap_err(),
                WorkDirError::WrongMode,
                "{mode:o}"
            );
            assert_eq!(mode_of(&path), mode, "it must not be changed");
        }
    }

    #[test]
    fn a_folder_of_another_owner_is_refused() {
        let temp = Temp::new("owner");
        let path = temp.join("work");
        make_dir(&path, 0o700);
        assert_eq!(
            WorkDir::open(&path, me() + 1).unwrap_err(),
            WorkDirError::WrongOwner
        );
    }

    #[test]
    fn a_missing_parent_is_an_io_error_and_nothing_is_created() {
        let temp = Temp::new("parent");
        let path = temp.join("missing-parent").join("work");
        assert_eq!(WorkDir::open(&path, me()).unwrap_err(), WorkDirError::Io);
        assert!(!temp.join("missing-parent").exists());
    }

    // ---- output files -------------------------------------------------------------------------------------

    #[test]
    fn a_running_process_is_alive_and_an_impossible_id_is_not() {
        assert!(process_is_alive(std::process::id()));
        assert!(!process_is_alive(u32::MAX));
    }

    #[test]
    fn our_own_pid_does_not_count_as_another_live_process() {
        assert!(!another_process_is_alive(std::process::id()));
        assert!(another_process_is_alive(1));
        assert!(!another_process_is_alive(u32::MAX));
    }

    #[test]
    fn a_leftover_with_our_own_pid_is_removed_so_a_reused_pid_cannot_block_the_job() {
        let temp = Temp::new("reused-pid");
        let dir = WorkDir::open(&temp.join("work"), me()).unwrap();
        let own = std::process::id();
        for extension in ["out", "err"] {
            fs::write(dir.path().join(file_name(own, 0, extension)), b"x").unwrap();
        }
        assert_eq!(dir.remove_leftovers(&another_process_is_alive), 2);
        // ... and the first output files of this run can now be created.
        assert!(dir.create_output_files(own, 0).is_ok());
    }

    #[test]
    fn the_file_names_are_built_from_numbers_only() {
        assert_eq!(file_name(123, 4, "out"), "tidewise-helper-123-4.out");
        assert_eq!(file_name(1, 0, "err"), "tidewise-helper-1-0.err");
    }

    #[test]
    fn the_two_output_files_are_created_empty_with_mode_0600() {
        let temp = Temp::new("files");
        let dir = WorkDir::open(&temp.join("work"), me()).unwrap();
        let files = dir.create_output_files(77, 3).unwrap();
        assert_eq!(files.out_path, dir.path().join("tidewise-helper-77-3.out"));
        assert_eq!(files.err_path, dir.path().join("tidewise-helper-77-3.err"));
        for path in [&files.out_path, &files.err_path] {
            assert!(fs::symlink_metadata(path).unwrap().is_file());
            assert_eq!(fs::metadata(path).unwrap().len(), 0);
            assert_eq!(mode_of(path), 0o600);
        }
    }

    #[test]
    fn an_existing_name_is_never_reused_or_followed() {
        let temp = Temp::new("exists");
        let dir = WorkDir::open(&temp.join("work"), me()).unwrap();
        dir.create_output_files(77, 3).unwrap();
        assert_eq!(
            dir.create_output_files(77, 3).unwrap_err(),
            WorkDirError::Io
        );

        // A symlink planted at the name (by root only, in a 0700 folder, but still): refused, target untouched.
        let target = temp.join("target");
        fs::write(&target, b"precious").unwrap();
        symlink(&target, dir.path().join("tidewise-helper-77-9.out")).unwrap();
        assert!(dir.create_output_files(77, 9).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"precious");
    }

    #[test]
    fn a_failed_second_file_leaves_no_half_pair_behind() {
        let temp = Temp::new("half");
        let dir = WorkDir::open(&temp.join("work"), me()).unwrap();
        fs::write(dir.path().join("tidewise-helper-5-1.err"), b"").unwrap();
        assert!(dir.create_output_files(5, 1).is_err());
        assert!(!dir.path().join("tidewise-helper-5-1.out").exists());
    }

    // ---- leftovers ----------------------------------------------------------------------------------------

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn leftovers_of_dead_helpers_are_removed_and_nothing_else() {
        let temp = Temp::new("leftovers");
        let dir = WorkDir::open(&temp.join("work"), me()).unwrap();
        let root = dir.path();
        // Ours, dead (pid 111) and alive (pid 222):
        for name in [
            "tidewise-helper-111-0.out",
            "tidewise-helper-111-0.err",
            "tidewise-helper-111-12.out",
            "tidewise-helper-222-0.out",
            "tidewise-helper-222-0.err",
        ] {
            fs::write(root.join(name), b"x").unwrap();
        }
        // Not ours, or not exactly the pattern: all must stay.
        for name in [
            "other.txt",
            "tidewise-helper-111-0.txt",
            "tidewise-helper-111-0.out.bak",
            "tidewise-helper-x-0.out",
            "tidewise-helper--0.out",
            "tidewise-helper-111-.out",
            "tidewise-helper-111.out",
            "tidewise-helper-111-0-1.out",
            "Tidewise-helper-111-0.out",
            "tidewise-helper-99999999999-0.out",
            ".tidewise-helper-111-0.out",
        ] {
            fs::write(root.join(name), b"x").unwrap();
        }
        // A directory and a symlink named like ours are not regular files: left alone.
        fs::create_dir(root.join("tidewise-helper-111-1.out")).unwrap();
        symlink(
            temp.join("elsewhere"),
            root.join("tidewise-helper-111-2.out"),
        )
        .unwrap();

        let alive = |pid: u32| pid == 222;
        assert_eq!(dir.remove_leftovers(&alive), 3);

        let left = names_in(root);
        for gone in [
            "tidewise-helper-111-0.out",
            "tidewise-helper-111-0.err",
            "tidewise-helper-111-12.out",
        ] {
            assert!(!left.contains(&gone.to_string()), "{gone} should be gone");
        }
        for kept in [
            "tidewise-helper-222-0.out",
            "tidewise-helper-222-0.err",
            "other.txt",
            "tidewise-helper-111-0.txt",
            "tidewise-helper-111-0.out.bak",
            "tidewise-helper-x-0.out",
            "tidewise-helper-111-1.out",
            "tidewise-helper-111-2.out",
            ".tidewise-helper-111-0.out",
        ] {
            assert!(
                left.contains(&kept.to_string()),
                "{kept} must stay: {left:?}"
            );
        }
    }

    #[test]
    fn leftovers_in_an_empty_folder_are_nothing() {
        let temp = Temp::new("empty");
        let dir = WorkDir::open(&temp.join("work"), me()).unwrap();
        assert_eq!(dir.remove_leftovers(&|_| false), 0);
    }

    #[test]
    fn many_leftovers_are_all_removed() {
        let temp = Temp::new("many");
        let dir = WorkDir::open(&temp.join("work"), me()).unwrap();
        for n in 0..2000 {
            fs::write(dir.path().join(file_name(4000, n, "out")), b"").unwrap();
        }
        assert_eq!(dir.remove_leftovers(&|_| false), 2000);
        assert!(names_in(dir.path()).is_empty());
    }
}
