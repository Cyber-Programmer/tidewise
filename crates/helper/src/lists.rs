//! The private copy of the package lists (decision 0007, review 5.8 R-02, golden rules 4, 7 and 9).
//!
//! An update syncs a private `--dbpath` in `/run/tidewise-helper-lists/` and compares there. Only when the list matches
//! what the user confirmed does `move_in` copy the refreshed lists into the real `/var/lib/pacman/sync/`. Until then the
//! real lists are not touched, so a refused update leaves the system exactly as it was.
//!
//! STUB (6.10b, red commit): the types and signatures are final, the behaviour is not written yet.

use std::path::PathBuf;

/// `--dbpath` of the private sync, with the trailing slash pacman wants.
pub const LISTS_DBPATH: &str = "/run/tidewise-helper-lists/";
/// The folder without the slash.
pub const LISTS_DIR: &str = "/run/tidewise-helper-lists";
/// pacman's real sync folder.
pub const REAL_SYNC_DIR: &str = "/var/lib/pacman/sync";
/// The real local database, which the private folder links to.
pub const REAL_LOCAL_DIR: &str = "/var/lib/pacman/local";
/// The real lock. Only looked at, never created, changed or removed (golden rule 9).
pub const REAL_LOCK: &str = "/var/lib/pacman/db.lck";

/// Why the private copy could not be prepared or moved in. A code only; the flow turns it into a fixed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListsError {
    /// The private folder, its `sync/` or its `local` link is not what the helper requires (owner, mode, kind, target).
    FolderUnusable,
    /// The real `db.lck` is there (looked at, before staging or right before the renames).
    Locked,
    /// The real folder has `<repo>.db.sig` and the new copy has none: the repository stopped signing its database.
    StoppedSigning,
    /// A refreshed list is not a regular file owned by the expected user.
    BadFile,
    /// Any other failure while reading or writing.
    Io,
}

/// A failed move: why, and how many lists had already been renamed into place (0 means the real lists are untouched).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveError {
    pub error: ListsError,
    pub renamed: u32,
}

/// What `move_in` did, in order. Only the unit tests look at it (`move_traced`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A look at the real `db.lck`, just before it is done.
    LockLook,
    /// A staged copy written (`.tidewise-<name>.new` in the real folder).
    Stage(String),
    /// The staged copy flushed to disk.
    SyncFile(String),
    /// One rename of a staged copy over the real list. Both paths are in the real `sync/`.
    Rename { from: PathBuf, to: PathBuf },
    /// The real `sync/` folder flushed to disk.
    SyncDir,
}

/// The places the private copy uses. `Lists::standard()` is the only way to get one outside the test builds.
pub struct Lists {
    private: PathBuf,
    real_sync: PathBuf,
    real_lock: PathBuf,
    local_target: PathBuf,
    owner: u32,
}

impl Lists {
    pub fn standard() -> Lists {
        Lists {
            private: PathBuf::from(LISTS_DIR),
            real_sync: PathBuf::from(REAL_SYNC_DIR),
            real_lock: PathBuf::from(REAL_LOCK),
            local_target: PathBuf::from(REAL_LOCAL_DIR),
            owner: 0,
        }
    }

    /// Folders inside `base`, owned by `owner`, for the unit tests.
    #[cfg(test)]
    pub(crate) fn in_folder(base: &std::path::Path, owner: u32) -> Lists {
        Lists {
            private: base.join("run-lists"),
            real_sync: base.join("var-lib-pacman").join("sync"),
            real_lock: base.join("var-lib-pacman").join("db.lck"),
            local_target: base.join("var-lib-pacman").join("local"),
            owner,
        }
    }

    /// Creates the private folder, its `sync/` and the `local` link when they are missing (mode 0711), and checks all
    /// three again. Anything that is there and is not as required is refused, never repaired.
    pub fn prepare(&self) -> Result<(), ListsError> {
        let _ = (&self.private, &self.local_target, self.owner);
        Err(ListsError::Io)
    }

    /// True if the private `db.lck` is there, or cannot be checked. A look only.
    pub fn private_lock_present(&self) -> bool {
        let _ = &self.private;
        false
    }

    /// Copies the refreshed lists into the real `sync/` (see decision 0007, "The move, in this order").
    pub fn move_in(&self) -> Result<(), MoveError> {
        self.move_traced(&mut |_| {})
    }

    fn move_traced(&self, _trace: &mut dyn FnMut(Step)) -> Result<(), MoveError> {
        let _ = (&self.real_sync, &self.real_lock);
        Err(MoveError {
            error: ListsError::Io,
            renamed: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::path::Path;

    fn mode_of(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().mode() & 0o7777
    }

    fn put(path: &Path, text: &str) {
        fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    /// The names and contents of the files directly in `dir` (folders by name), to compare before and after.
    fn listing(dir: &Path) -> Vec<String> {
        let mut lines: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type().unwrap().is_file() {
                    format!("{name}={}", read(&entry.path()))
                } else {
                    format!("{name}/")
                }
            })
            .collect();
        lines.sort();
        lines
    }

    struct World {
        temp: TempDir,
        lists: Lists,
    }

    impl World {
        /// Nothing of ours exists yet; the real side (sync folder, local database) does.
        fn empty(test: &str) -> World {
            let temp = TempDir::new(test);
            let real = temp.join("var-lib-pacman");
            fs::create_dir_all(real.join("sync")).unwrap();
            fs::create_dir_all(real.join("local")).unwrap();
            let lists = Lists::in_folder(&temp.0, me_for(&temp));
            World { temp, lists }
        }

        /// The private folder built by hand (not through `prepare`), with these lists in its `sync/`.
        fn with_private(test: &str, private: &[(&str, &str)]) -> World {
            let world = World::empty(test);
            let dir = world.private();
            fs::create_dir(&dir).unwrap();
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o711)).unwrap();
            symlink(
                world.temp.join("var-lib-pacman").join("local"),
                dir.join("local"),
            )
            .unwrap();
            fs::create_dir(dir.join("sync")).unwrap();
            fs::set_permissions(dir.join("sync"), fs::Permissions::from_mode(0o711)).unwrap();
            for (name, text) in private {
                put(&dir.join("sync").join(name), text);
            }
            world
        }

        fn private(&self) -> PathBuf {
            self.temp.join("run-lists")
        }
        fn private_sync(&self) -> PathBuf {
            self.private().join("sync")
        }
        fn real_sync(&self) -> PathBuf {
            self.temp.join("var-lib-pacman").join("sync")
        }
        fn real_lock(&self) -> PathBuf {
            self.temp.join("var-lib-pacman").join("db.lck")
        }
        fn real(&self, name: &str, text: &str) {
            put(&self.real_sync().join(name), text);
        }
        fn trace(&self) -> (Result<(), MoveError>, Vec<Step>) {
            let mut steps = Vec::new();
            let result = self.lists.move_traced(&mut |step| steps.push(step));
            (result, steps)
        }
    }

    fn me_for(temp: &TempDir) -> u32 {
        fs::metadata(&temp.0).unwrap().uid()
    }

    fn untouched(error: ListsError) -> Result<(), MoveError> {
        Err(MoveError { error, renamed: 0 })
    }

    // ---- the folder: first run, and a planted one ---------------------------------------------------------

    #[test]
    fn created_on_first_run_with_exact_mode() {
        let world = World::empty("first-run");
        assert_eq!(world.lists.prepare(), Ok(()));
        assert_eq!(mode_of(&world.private()), 0o711);
        assert_eq!(mode_of(&world.private_sync()), 0o711);
        assert_eq!(
            fs::metadata(world.private()).unwrap().uid(),
            me_for(&world.temp)
        );
        let link = world.private().join("local");
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_link(&link).unwrap(),
            world.temp.join("var-lib-pacman").join("local")
        );
        // Nothing else was made: no lock, nothing in the real folders.
        assert_eq!(listing(&world.private_sync()), Vec::<String>::new());
        assert!(!world.private().join("db.lck").exists());
    }

    #[test]
    fn a_second_prepare_finds_everything_in_order_and_changes_nothing() {
        let world = World::empty("second-run");
        assert_eq!(world.lists.prepare(), Ok(()));
        put(&world.private_sync().join("core.db"), "kept");
        assert_eq!(world.lists.prepare(), Ok(()));
        assert_eq!(read(&world.private_sync().join("core.db")), "kept");
    }

    /// Each case plants something wrong, then `prepare` must refuse and must not repair it.
    #[test]
    fn a_planted_folder_is_refused() {
        type Plant = fn(&World);
        let cases: Vec<(&str, Plant)> = vec![
            ("private folder 0755", |w| {
                fs::create_dir(w.private()).unwrap();
                fs::set_permissions(w.private(), fs::Permissions::from_mode(0o755)).unwrap();
            }),
            ("private folder 0700", |w| {
                fs::create_dir(w.private()).unwrap();
                fs::set_permissions(w.private(), fs::Permissions::from_mode(0o700)).unwrap();
            }),
            ("private folder 0777", |w| {
                fs::create_dir(w.private()).unwrap();
                fs::set_permissions(w.private(), fs::Permissions::from_mode(0o777)).unwrap();
            }),
            ("private folder is a symlink", |w| {
                let elsewhere = w.temp.join("elsewhere");
                fs::create_dir(&elsewhere).unwrap();
                fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o711)).unwrap();
                symlink(&elsewhere, w.private()).unwrap();
            }),
            ("private folder is a file", |w| put(&w.private(), "x")),
            ("sync/ is 0755", |w| {
                w.lists.prepare().unwrap();
                fs::set_permissions(w.private_sync(), fs::Permissions::from_mode(0o755)).unwrap();
            }),
            ("sync/ is a symlink", |w| {
                w.lists.prepare().unwrap();
                fs::remove_dir(w.private_sync()).unwrap();
                symlink(w.real_sync(), w.private_sync()).unwrap();
            }),
            ("sync/ is a file", |w| {
                w.lists.prepare().unwrap();
                fs::remove_dir(w.private_sync()).unwrap();
                put(&w.private_sync(), "x");
            }),
            ("local points somewhere else", |w| {
                w.lists.prepare().unwrap();
                fs::remove_file(w.private().join("local")).unwrap();
                symlink("/etc", w.private().join("local")).unwrap();
            }),
            ("local is a file", |w| {
                w.lists.prepare().unwrap();
                fs::remove_file(w.private().join("local")).unwrap();
                put(&w.private().join("local"), "x");
            }),
            ("local is a real folder", |w| {
                w.lists.prepare().unwrap();
                fs::remove_file(w.private().join("local")).unwrap();
                fs::create_dir(w.private().join("local")).unwrap();
            }),
        ];
        for (name, plant) in cases {
            let world = World::empty("planted");
            plant(&world);
            let before = (
                fs::symlink_metadata(world.private())
                    .map(|m| (m.mode(), m.file_type().is_symlink())),
                fs::symlink_metadata(world.private_sync())
                    .map(|m| (m.mode(), m.file_type().is_symlink())),
                fs::read_link(world.private().join("local")).ok(),
            );
            assert_eq!(
                world.lists.prepare(),
                Err(ListsError::FolderUnusable),
                "{name}"
            );
            let after = (
                fs::symlink_metadata(world.private())
                    .map(|m| (m.mode(), m.file_type().is_symlink())),
                fs::symlink_metadata(world.private_sync())
                    .map(|m| (m.mode(), m.file_type().is_symlink())),
                fs::read_link(world.private().join("local")).ok(),
            );
            assert_eq!(
                format!("{before:?}"),
                format!("{after:?}"),
                "{name}: it was repaired"
            );
        }
    }

    #[test]
    fn a_folder_owned_by_someone_else_is_refused() {
        let world = World::empty("owner");
        assert_eq!(world.lists.prepare(), Ok(()));
        let strict = Lists::in_folder(&world.temp.0, me_for(&world.temp) + 1);
        assert_eq!(strict.prepare(), Err(ListsError::FolderUnusable));
    }

    #[test]
    fn the_private_lock_is_only_looked_at() {
        let world = World::with_private("private-lock", &[]);
        assert!(!world.lists.private_lock_present());
        let lock = world.private().join("db.lck");
        put(&lock, "");
        assert!(world.lists.private_lock_present());
        // A dangling symlink is "there" too: the look does not follow it.
        fs::remove_file(&lock).unwrap();
        symlink(world.temp.join("nowhere"), &lock).unwrap();
        assert!(world.lists.private_lock_present());
        assert!(
            fs::symlink_metadata(&lock).is_ok(),
            "the look removed the lock"
        );
    }

    // ---- the move ---------------------------------------------------------------------------------------

    #[test]
    fn a_refreshed_list_replaces_the_real_one_and_is_a_copy() {
        let world = World::with_private(
            "replace",
            &[("core.db", "new core"), ("extra.db", "new extra")],
        );
        world.real("core.db", "old core");
        world.real("extra.db", "old extra");
        let (result, _) = world.trace();
        assert_eq!(result, Ok(()));
        assert_eq!(read(&world.real_sync().join("core.db")), "new core");
        assert_eq!(read(&world.real_sync().join("extra.db")), "new extra");
        assert_eq!(mode_of(&world.real_sync().join("core.db")), 0o644);
        // A copy: the private lists stay (the next private sync uses them), and nothing is left over in the real folder.
        assert_eq!(read(&world.private_sync().join("core.db")), "new core");
        assert_eq!(listing(&world.real_sync()).len(), 2);
        assert!(!world.real_lock().exists(), "a lock was made");
    }

    #[test]
    fn only_allowed_names_move_and_the_rest_is_ignored() {
        let world = World::with_private(
            "names",
            &[
                ("core.db", "core"),
                ("notes.txt", "x"),
                ("core.db.part", "x"),
                (".hidden.db", "x"),
                ("-flag.db", "x"),
                ("local.db", "x"),
                ("Ünï.db", "x"),
            ],
        );
        fs::create_dir(world.private_sync().join("download-AbCdEf")).unwrap();
        let before = listing(&world.private_sync());
        let (result, _) = world.trace();
        assert_eq!(result, Ok(()));
        assert_eq!(
            listing(&world.real_sync()),
            vec!["core.db=core".to_string()]
        );
        assert_eq!(
            listing(&world.private_sync()),
            before,
            "the private folder changed"
        );
    }

    #[test]
    fn a_symlink_a_wrong_owner_and_a_non_file_are_refused_before_anything_is_written() {
        // symlink
        let world = World::with_private("symlink", &[("extra.db", "extra")]);
        put(&world.temp.join("target"), "elsewhere");
        symlink(
            world.temp.join("target"),
            world.private_sync().join("core.db"),
        )
        .unwrap();
        world.real("core.db", "old");
        let before = listing(&world.real_sync());
        assert_eq!(world.trace().0, untouched(ListsError::BadFile));
        assert_eq!(listing(&world.real_sync()), before);

        // a folder with a list's name
        let world = World::with_private("folder", &[("extra.db", "extra")]);
        fs::create_dir(world.private_sync().join("core.db")).unwrap();
        assert_eq!(world.trace().0, untouched(ListsError::BadFile));
        assert_eq!(listing(&world.real_sync()), Vec::<String>::new());

        // a file of another user
        let world = World::with_private("owner", &[("core.db", "core")]);
        let strict = Lists::in_folder(&world.temp.0, me_for(&world.temp) + 1);
        assert_eq!(strict.move_in(), untouched(ListsError::BadFile));
        assert_eq!(listing(&world.real_sync()), Vec::<String>::new());
    }

    #[test]
    fn all_lists_are_staged_and_synced_before_any_is_renamed_then_the_folder_is_synced() {
        let world = World::with_private("order", &[("core.db", "a"), ("extra.db", "b")]);
        world.real("core.db", "old");
        let (result, steps) = world.trace();
        assert_eq!(result, Ok(()));
        let real = world.real_sync();
        assert_eq!(
            steps,
            vec![
                Step::LockLook,
                Step::Stage("core.db".to_string()),
                Step::SyncFile("core.db".to_string()),
                Step::Stage("extra.db".to_string()),
                Step::SyncFile("extra.db".to_string()),
                Step::LockLook,
                Step::Rename {
                    from: real.join(".tidewise-core.db.new"),
                    to: real.join("core.db")
                },
                Step::Rename {
                    from: real.join(".tidewise-extra.db.new"),
                    to: real.join("extra.db")
                },
                Step::SyncDir,
            ]
        );
    }

    #[test]
    fn a_rename_never_crosses_folders() {
        // /run (tmpfs) and /var/lib/pacman (ext4) are different filesystems on a real system: rename(2) between them
        // fails with EXDEV (measured on the host, decision 0007). The design never does that: staged file and target
        // are always names inside the one real sync folder.
        let world = World::with_private(
            "same-parent",
            &[("core.db", "a"), ("core.db.sig", "s"), ("extra.db", "b")],
        );
        let (result, steps) = world.trace();
        assert_eq!(result, Ok(()));
        let renames: Vec<(PathBuf, PathBuf)> = steps
            .into_iter()
            .filter_map(|step| match step {
                Step::Rename { from, to } => Some((from, to)),
                _ => None,
            })
            .collect();
        assert_eq!(renames.len(), 3);
        for (from, to) in renames {
            assert_eq!(from.parent(), Some(world.real_sync().as_path()), "{from:?}");
            assert_eq!(to.parent(), Some(world.real_sync().as_path()), "{to:?}");
        }
    }

    #[test]
    fn a_real_lock_means_nothing_is_staged() {
        let world = World::with_private("lock-first", &[("core.db", "new")]);
        world.real("core.db", "old");
        put(&world.real_lock(), "");
        let before = listing(&world.real_sync());
        let (result, steps) = world.trace();
        assert_eq!(result, untouched(ListsError::Locked));
        assert_eq!(listing(&world.real_sync()), before, "something was staged");
        assert_eq!(steps, vec![Step::LockLook]);
        assert_eq!(read(&world.real_lock()), "", "the lock was changed");
    }

    #[test]
    fn a_lock_that_appears_before_the_renames_means_nothing_is_renamed() {
        let world = World::with_private("lock-late", &[("core.db", "new"), ("extra.db", "new")]);
        world.real("core.db", "old core");
        world.real("extra.db", "old extra");
        let lock = world.real_lock();
        let mut looks = 0;
        let mut steps = Vec::new();
        let result = world.lists.move_traced(&mut |step| {
            if step == Step::LockLook {
                looks += 1;
                if looks == 2 {
                    put(&lock, ""); // another package manager starts while the copies are being staged
                }
            }
            steps.push(step);
        });
        assert_eq!(result, untouched(ListsError::Locked));
        assert_eq!(read(&world.real_sync().join("core.db")), "old core");
        assert_eq!(read(&world.real_sync().join("extra.db")), "old extra");
        assert!(!steps.iter().any(|s| matches!(s, Step::Rename { .. })));
        // The staged copies stay (pacman -Sc removes them; the next run reuses them): nothing is deleted.
        assert!(world.real_sync().join(".tidewise-core.db.new").exists());
        assert_eq!(read(&lock), "");
    }

    #[test]
    fn a_staged_name_that_is_a_symlink_is_not_followed() {
        let world = World::with_private("staged-symlink", &[("core.db", "new")]);
        world.real("core.db", "old");
        put(&world.temp.join("outside"), "keep me");
        symlink(
            world.temp.join("outside"),
            world.real_sync().join(".tidewise-core.db.new"),
        )
        .unwrap();
        assert_eq!(world.trace().0, untouched(ListsError::Io));
        assert_eq!(read(&world.temp.join("outside")), "keep me");
        assert_eq!(read(&world.real_sync().join("core.db")), "old");
    }

    #[test]
    fn a_leftover_staged_copy_is_reused() {
        let world = World::with_private("leftover", &[("core.db", "new")]);
        world.real("core.db", "old");
        world.real(
            ".tidewise-core.db.new",
            "a longer leftover from a killed update",
        );
        assert_eq!(world.trace().0, Ok(()));
        assert_eq!(read(&world.real_sync().join("core.db")), "new");
        assert_eq!(listing(&world.real_sync()), vec!["core.db=new".to_string()]);
    }

    // ---- signatures (decision 0007, "Signatures") ------------------------------------------------------

    #[test]
    fn a_private_signature_moves_with_its_list_over_an_old_one() {
        let world = World::with_private(
            "sig-both",
            &[("core.db", "new"), ("core.db.sig", "new sig")],
        );
        world.real("core.db", "old");
        world.real("core.db.sig", "old sig");
        assert_eq!(world.trace().0, Ok(()));
        assert_eq!(read(&world.real_sync().join("core.db")), "new");
        assert_eq!(read(&world.real_sync().join("core.db.sig")), "new sig");
    }

    #[test]
    fn a_private_signature_moves_when_the_real_folder_had_none() {
        let world =
            World::with_private("sig-new", &[("core.db", "new"), ("core.db.sig", "new sig")]);
        world.real("core.db", "old");
        assert_eq!(world.trace().0, Ok(()));
        assert_eq!(read(&world.real_sync().join("core.db.sig")), "new sig");
    }

    #[test]
    fn with_no_signature_on_either_side_only_the_list_moves() {
        let world = World::with_private("sig-none", &[("core.db", "new")]);
        world.real("core.db", "old");
        assert_eq!(world.trace().0, Ok(()));
        assert_eq!(listing(&world.real_sync()), vec!["core.db=new".to_string()]);
    }

    #[test]
    fn a_repository_that_stopped_signing_is_refused_before_anything_is_staged() {
        let world = World::with_private("sig-stopped", &[("core.db", "new"), ("extra.db", "new")]);
        world.real("core.db", "old core");
        world.real("core.db.sig", "old sig");
        world.real("extra.db", "old extra");
        let before = listing(&world.real_sync());
        let (result, steps) = world.trace();
        assert_eq!(result, untouched(ListsError::StoppedSigning));
        assert_eq!(
            listing(&world.real_sync()),
            before,
            "something was staged or changed"
        );
        assert!(
            !steps
                .iter()
                .any(|s| matches!(s, Step::Stage(_) | Step::Rename { .. }))
        );
    }
}
