//! The private copy of the package lists (decision 0007, review 5.8 R-02, golden rules 4, 7 and 9).
//!
//! An update syncs a private `--dbpath` in `/run/tidewise-helper-lists/` and compares there. Only when the list matches
//! what the user confirmed does `move_in` copy the refreshed lists into the real `/var/lib/pacman/sync/`. Until then the
//! real lists are not touched, so a refused update leaves the system exactly as it was.
//!
//!
//! Nothing here deletes anything. The one `rename` replaces a list that is cache-class data (golden rule 7), inside the
//! real `sync/` folder only. Every look at a `db.lck` is a `symlink_metadata` (golden rule 9).

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, ErrorKind};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use rustix::fs::OFlags;
use tidewise_names::is_helper_repo_name;

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
/// The lock's name inside the private folder. Same rule: only looked at.
const LOCK_NAME: &str = "db.lck";

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
#[derive(Debug)]
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

    /// Creates the private folder, its `sync/` and the `local` link when they are missing (mode 0711), and checks all
    /// three again. Anything that is there and is not as required is refused, never repaired.
    pub fn prepare(&self) -> Result<(), ListsError> {
        self.folder(&self.private)?;
        self.folder(&self.private.join("sync"))?;
        self.local_link()
    }

    /// A real folder (not a link), owned by `owner`, with exactly mode 0711. Made if it is missing; then looked at again
    /// whoever made it, because `mkdir` is subject to the umask and the folder may have been made in between.
    fn folder(&self, path: &Path) -> Result<(), ListsError> {
        match fs::symlink_metadata(path) {
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                match DirBuilder::new().mode(0o711).create(path) {
                    Ok(()) => {}
                    // Made between our look and our create: judged below like any other.
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(ListsError::Io),
                }
            }
            Err(_) => return Err(ListsError::Io),
        }
        let meta = fs::symlink_metadata(path).map_err(|_| ListsError::Io)?;
        if meta.is_dir() && meta.uid() == self.owner && meta.mode() & 0o7777 == 0o711 {
            Ok(())
        } else {
            Err(ListsError::FolderUnusable)
        }
    }

    /// `local` is a link to the real local database (the same recipe as checkupdates): made when missing, and when it
    /// exists it must be exactly that link.
    fn local_link(&self) -> Result<(), ListsError> {
        let link = self.private.join("local");
        match fs::symlink_metadata(&link) {
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {
                match symlink(&self.local_target, &link) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(ListsError::Io),
                }
            }
            Err(_) => return Err(ListsError::Io),
        }
        let meta = fs::symlink_metadata(&link).map_err(|_| ListsError::Io)?;
        let right_target = fs::read_link(&link).is_ok_and(|target| target == self.local_target);
        if meta.file_type().is_symlink() && meta.uid() == self.owner && right_target {
            Ok(())
        } else {
            Err(ListsError::FolderUnusable)
        }
    }

    /// True if the private `db.lck` is there, or cannot be checked. A look only: `symlink_metadata` never opens,
    /// creates or follows anything.
    pub fn private_lock_present(&self) -> bool {
        looks_present(&self.private.join(LOCK_NAME))
    }

    /// Copies the refreshed lists into the real `sync/` (decision 0007, "The move, in this order").
    pub fn move_in(&self) -> Result<u32, MoveError> {
        self.move_traced(&mut |_| {})
    }

    fn move_traced(&self, trace: &mut dyn FnMut(Step)) -> Result<u32, MoveError> {
        let nothing_moved = |error| MoveError { error, renamed: 0 };

        // 1. Every check that needs no write: names, kinds, owners, and the signature rule.
        let lists = self.refreshed_lists().map_err(nothing_moved)?;
        self.check_signatures(&lists).map_err(nothing_moved)?;

        // 2. The real lock, before anything is written.
        trace(Step::LockLook);
        if looks_present(&self.real_lock) {
            return Err(nothing_moved(ListsError::Locked));
        }

        // 3. Every copy is staged and flushed before the first rename.
        for list in &lists {
            self.stage(list, trace).map_err(nothing_moved)?;
        }

        // 4. Staging takes time: another package manager may have started. Look again, right before the renames.
        trace(Step::LockLook);
        if looks_present(&self.real_lock) {
            return Err(nothing_moved(ListsError::Locked));
        }

        // 5. One rename per list, inside the real sync folder.
        let mut renamed = 0;
        for list in &lists {
            let staged = staged_name(&list.name);
            if replace_list(&self.real_sync, &staged, &list.name).is_err() {
                return Err(MoveError {
                    error: ListsError::Io,
                    renamed,
                });
            }
            renamed += 1;
            trace(Step::Rename {
                from: self.real_sync.join(&staged),
                to: self.real_sync.join(&list.name),
            });
        }

        // 6. A synced file does not make its directory entry durable: flush the folder.
        let flushed = File::open(&self.real_sync).and_then(|folder| folder.sync_all());
        if flushed.is_err() {
            return Err(MoveError {
                error: ListsError::Io,
                renamed,
            });
        }
        trace(Step::SyncDir);
        Ok(renamed)
    }

    /// The lists in the private `sync/` that may move: `<repo>.db` and `<repo>.db.sig` with a repository name that
    /// passes the allow-list. Everything else (the `download-*` folders, odd names) is ignored, never read. A list that
    /// is not a regular file owned by `owner` refuses the whole move. Sorted by name.
    fn refreshed_lists(&self) -> Result<Vec<List>, ListsError> {
        let entries = fs::read_dir(self.private.join("sync")).map_err(|_| ListsError::Io)?;
        let mut lists = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| ListsError::Io)?;
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let Some((repo, is_signature)) = split_list_name(&name) else {
                continue;
            };
            if !is_helper_repo_name(repo) {
                continue;
            }
            let meta = fs::symlink_metadata(entry.path()).map_err(|_| ListsError::Io)?;
            if !meta.file_type().is_file() || meta.uid() != self.owner {
                return Err(ListsError::BadFile);
            }
            lists.push(List {
                repo: repo.to_string(),
                is_signature,
                name,
            });
        }
        lists.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(lists)
    }

    /// pacman leaves a stale `<repo>.db.sig` next to a new list and then cannot read the pair (measured, decision 0007).
    /// So a repository whose new copy has no signature while the real folder has one is refused, before any write.
    fn check_signatures(&self, lists: &[List]) -> Result<(), ListsError> {
        for list in lists.iter().filter(|list| !list.is_signature) {
            let has_signature = lists
                .iter()
                .any(|other| other.is_signature && other.repo == list.repo);
            if has_signature {
                continue;
            }
            let real_signature = self.real_sync.join(format!("{}.db.sig", list.repo));
            if looks_present(&real_signature) {
                return Err(ListsError::StoppedSigning);
            }
        }
        Ok(())
    }

    /// Copies one private list to `.tidewise-<name>.new` in the real folder and flushes it. The private file is opened
    /// without following a link and judged again on the open descriptor; the staged file is created without following a
    /// link, truncated if a killed update left one, and given mode 0644 whatever the umask or the leftover said.
    fn stage(&self, list: &List, trace: &mut dyn FnMut(Step)) -> Result<(), ListsError> {
        let no_follow = OFlags::NOFOLLOW.bits() as i32;
        let mut source = OpenOptions::new()
            .read(true)
            .custom_flags(no_follow)
            .open(self.private.join("sync").join(&list.name))
            .map_err(|_| ListsError::Io)?;
        let meta = source.metadata().map_err(|_| ListsError::Io)?;
        if !meta.file_type().is_file() || meta.uid() != self.owner {
            return Err(ListsError::BadFile);
        }
        let mut staged = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .custom_flags(no_follow)
            .open(self.real_sync.join(staged_name(&list.name)))
            .map_err(|_| ListsError::Io)?;
        staged
            .set_permissions(fs::Permissions::from_mode(0o644))
            .map_err(|_| ListsError::Io)?;
        io::copy(&mut source, &mut staged).map_err(|_| ListsError::Io)?;
        // pacman sets a downloaded list's time to the server's and sends it back as If-Modified-Since: keep it, or a list
        // published between the server's time and this copy would look unmodified to the next plain `pacman -Sy`.
        staged
            .set_modified(meta.modified().map_err(|_| ListsError::Io)?)
            .map_err(|_| ListsError::Io)?;
        trace(Step::Stage(list.name.clone()));
        staged.sync_all().map_err(|_| ListsError::Io)?;
        trace(Step::SyncFile(list.name.clone()));
        Ok(())
    }
}

/// One list that may move.
struct List {
    repo: String,
    is_signature: bool,
    /// `<repo>.db` or `<repo>.db.sig`.
    name: String,
}

/// `core.db` is `("core", false)`, `core.db.sig` is `("core", true)`, anything else is `None`.
fn split_list_name(name: &str) -> Option<(&str, bool)> {
    if let Some(repo) = name.strip_suffix(".db") {
        Some((repo, false))
    } else {
        name.strip_suffix(".db.sig").map(|repo| (repo, true))
    }
}

fn staged_name(list_name: &str) -> String {
    format!(".tidewise-{list_name}.new")
}

/// True if something is at `path`, or it cannot be checked (unknown counts as present). A look: it follows nothing.
fn looks_present(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) => error.kind() != ErrorKind::NotFound,
    }
}

/// The helper's one rename: a staged list over the real one. Both names are inside the one folder `dir` by
/// construction (a rename across folders fails on a real system: `/run` is tmpfs, `/var` is not), and the staged name
/// is the helper's own dotfile.
#[expect(
    clippy::disallowed_methods,
    reason = "decision 0007: one rename of a staged pacman list over the real one inside the real sync folder; cache-class data, not a user file (golden rule 7)"
)]
fn replace_list(dir: &Path, staged_name: &str, list_name: &str) -> io::Result<()> {
    fs::rename(dir.join(staged_name), dir.join(list_name))
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
            let lists = in_folder(&temp.0, me_for(&temp));
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
        fn trace(&self) -> (Result<u32, MoveError>, Vec<Step>) {
            let mut steps = Vec::new();
            let result = self.lists.move_traced(&mut |step| steps.push(step));
            (result, steps)
        }
    }

    /// Folders inside `base`, owned by `owner`.
    fn in_folder(base: &Path, owner: u32) -> Lists {
        Lists {
            private: base.join("run-lists"),
            real_sync: base.join("var-lib-pacman").join("sync"),
            real_lock: base.join("var-lib-pacman").join("db.lck"),
            local_target: base.join("var-lib-pacman").join("local"),
            owner,
        }
    }

    fn me_for(temp: &TempDir) -> u32 {
        fs::metadata(&temp.0).unwrap().uid()
    }

    /// The private folder alone, at `mode` (no `sync/`, no link).
    fn by_hand(world: &World, mode: u32) {
        fs::create_dir(world.private()).unwrap();
        fs::set_permissions(world.private(), fs::Permissions::from_mode(mode)).unwrap();
    }

    /// A correct `sync/` inside the private folder.
    fn by_hand_sync(world: &World) {
        fs::create_dir(world.private_sync()).unwrap();
        fs::set_permissions(world.private_sync(), fs::Permissions::from_mode(0o711)).unwrap();
    }

    fn untouched(error: ListsError) -> Result<u32, MoveError> {
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
                by_hand(w, 0o711);
                fs::create_dir(w.private_sync()).unwrap();
                fs::set_permissions(w.private_sync(), fs::Permissions::from_mode(0o755)).unwrap();
            }),
            ("sync/ is a symlink", |w| {
                by_hand(w, 0o711);
                symlink(w.real_sync(), w.private_sync()).unwrap();
            }),
            ("sync/ is a file", |w| {
                by_hand(w, 0o711);
                put(&w.private_sync(), "x");
            }),
            ("local points somewhere else", |w| {
                by_hand(w, 0o711);
                by_hand_sync(w);
                symlink("/etc", w.private().join("local")).unwrap();
            }),
            ("local is a file", |w| {
                by_hand(w, 0o711);
                by_hand_sync(w);
                put(&w.private().join("local"), "x");
            }),
            ("local is a real folder", |w| {
                by_hand(w, 0o711);
                by_hand_sync(w);
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
        let strict = in_folder(&world.temp.0, me_for(&world.temp) + 1);
        assert_eq!(strict.prepare(), Err(ListsError::FolderUnusable));
    }

    #[test]
    fn the_private_lock_is_only_looked_at() {
        let world = World::with_private("private-lock", &[]);
        assert!(!world.lists.private_lock_present());
        let lock = world.private().join("db.lck");
        put(&lock, "marker");
        assert!(world.lists.private_lock_present());
        assert_eq!(read(&lock), "marker", "the look changed the lock");

        // A dangling symlink is "there" too: the look does not follow it, and does not remove it.
        let world = World::with_private("private-lock-link", &[]);
        let lock = world.private().join("db.lck");
        symlink(world.temp.join("nowhere"), &lock).unwrap();
        assert!(world.lists.private_lock_present());
        assert!(
            fs::symlink_metadata(&lock).is_ok(),
            "the look removed the lock"
        );
        assert!(
            !world.temp.join("nowhere").exists(),
            "the look created the target"
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
        assert_eq!(result, Ok(2));
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
        assert_eq!(result, Ok(1));
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
        let strict = in_folder(&world.temp.0, me_for(&world.temp) + 1);
        assert_eq!(strict.move_in(), untouched(ListsError::BadFile));
        assert_eq!(listing(&world.real_sync()), Vec::<String>::new());
    }

    #[test]
    fn all_lists_are_staged_and_synced_before_any_is_renamed_then_the_folder_is_synced() {
        let world = World::with_private("order", &[("core.db", "a"), ("extra.db", "b")]);
        world.real("core.db", "old");
        let (result, steps) = world.trace();
        assert_eq!(result, Ok(2));
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
        assert_eq!(result, Ok(3));
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
        assert_eq!(world.trace().0, Ok(1));
        assert_eq!(read(&world.real_sync().join("core.db")), "new");
        assert_eq!(listing(&world.real_sync()), vec!["core.db=new".to_string()]);
    }

    #[test]
    fn move_in_says_how_many_lists_it_renamed_and_none_when_there_was_nothing_to_move() {
        let world = World::with_private("count", &[("core.db", "a"), ("core.db.sig", "s")]);
        assert_eq!(world.lists.move_in(), Ok(2));
        // Only folders and odd names in the private sync/: nothing to move, nothing renamed, and it says so.
        let world = World::with_private("count-none", &[("notes.txt", "x")]);
        assert_eq!(world.lists.move_in(), Ok(0));
        assert_eq!(listing(&world.real_sync()), Vec::<String>::new());
    }

    #[test]
    fn a_staged_copy_keeps_the_time_of_the_refreshed_list() {
        // pacman sets a downloaded list's time to the server's and sends it back as If-Modified-Since. A copy that
        // took "now" would make a newer list, published between the server's time and the copy, look unmodified
        // to the user's next plain `pacman -Sy` (review 6.10b R-03).
        let world = World::with_private("mtime", &[("core.db", "new")]);
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let source = fs::OpenOptions::new()
            .write(true)
            .open(world.private_sync().join("core.db"))
            .unwrap();
        source.set_modified(old).unwrap();
        drop(source);
        world.lists.move_in().unwrap();
        let kept = fs::metadata(world.real_sync().join("core.db"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(kept, old);
    }

    #[test]
    fn a_leftover_with_another_mode_is_set_to_0644() {
        // The mode must not depend on what an old leftover (or the umask) says (mutation: no set_permissions).
        let world = World::with_private("leftover-mode", &[("core.db", "new")]);
        world.real(".tidewise-core.db.new", "old");
        fs::set_permissions(
            world.real_sync().join(".tidewise-core.db.new"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        world.lists.move_in().unwrap();
        assert_eq!(mode_of(&world.real_sync().join("core.db")), 0o644);
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
        assert_eq!(world.trace().0, Ok(2));
        assert_eq!(read(&world.real_sync().join("core.db")), "new");
        assert_eq!(read(&world.real_sync().join("core.db.sig")), "new sig");
    }

    #[test]
    fn a_private_signature_moves_when_the_real_folder_had_none() {
        let world =
            World::with_private("sig-new", &[("core.db", "new"), ("core.db.sig", "new sig")]);
        world.real("core.db", "old");
        assert_eq!(world.trace().0, Ok(2));
        assert_eq!(read(&world.real_sync().join("core.db.sig")), "new sig");
    }

    #[test]
    fn with_no_signature_on_either_side_only_the_list_moves() {
        let world = World::with_private("sig-none", &[("core.db", "new")]);
        world.real("core.db", "old");
        assert_eq!(world.trace().0, Ok(1));
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
