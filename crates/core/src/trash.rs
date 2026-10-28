//! Move to Trash and Undo, following the freedesktop Trash spec for the user's own Trash
//! (`$XDG_DATA_HOME/Trash` with `files/` and `info/`). Never deletes: a file that cannot be moved
//! (another filesystem, no permission) stays where it is and the error says so.
//!
//! Not done on purpose: the spec's per-drive `.Trash-<uid>` folders. They would write on drives the
//! user did not pick, so a file on another drive is refused with [`Error::OtherDrive`].

use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} does not exist")]
    NotFound(PathBuf),
    #[error("{0} is on a different drive than the Trash, so it cannot be moved there")]
    OtherDrive(PathBuf),
    #[error("{0} is protected and is never moved to the Trash")]
    Protected(PathBuf),
    #[error("{0} must be a full path")]
    NotAbsolute(PathBuf),
    #[error("{0} already exists, so the Undo would overwrite it")]
    Occupied(PathBuf),
    #[error("could not move {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// What was moved, enough to put it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trashed {
    pub original: PathBuf,
    pub in_trash: PathBuf,
    pub info: PathBuf,
}

/// `$XDG_DATA_HOME/Trash`, or `$HOME/.local/share/Trash`.
pub fn default_trash_dir() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("Trash"))
}

/// Move `path` into the Trash at `trash_dir`. `home` is never trashed.
pub fn move_to_trash(
    _path: &Path,
    _trash_dir: &Path,
    _home: &Path,
    _now: SystemTime,
) -> Result<Trashed, Error> {
    Err(Error::Protected(PathBuf::new()))
}

/// Put a trashed item back exactly where it was. Refuses to overwrite anything.
pub fn restore(_item: &Trashed) -> Result<(), Error> {
    Err(Error::Protected(PathBuf::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    /// Temp folders on the project's disk (not tmpfs), like `scan`'s tests.
    fn disk_dir() -> tempfile::TempDir {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp");
        fs::create_dir_all(&base).unwrap();
        tempfile::Builder::new().tempdir_in(base).unwrap()
    }

    struct Setup {
        _dir: tempfile::TempDir,
        home: PathBuf,
        trash: PathBuf,
    }

    fn setup() -> Setup {
        let dir = disk_dir();
        let home = dir.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let trash = dir.path().join("data/Trash");
        Setup {
            home,
            trash,
            _dir: dir,
        }
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_761_650_000)
    }

    #[test]
    fn a_file_goes_to_trash_and_undo_puts_it_back_exactly() {
        let s = setup();
        let f = s.home.join("Downloads/a.iso");
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        fs::write(&f, b"installer").unwrap();
        let t = move_to_trash(&f, &s.trash, &s.home, now()).unwrap();
        assert!(!f.exists());
        assert_eq!(fs::read(&t.in_trash).unwrap(), b"installer");
        let info = fs::read_to_string(&t.info).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath="), "{info}");
        assert!(info.contains("a.iso") && info.contains("DeletionDate=2025-10-28T"), "{info}");
        restore(&t).unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"installer");
        assert!(!t.info.exists() && !t.in_trash.exists());
    }

    #[test]
    fn a_folder_moves_with_everything_inside() {
        let s = setup();
        let d = s.home.join("datasets");
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("a/b/x.csv"), b"1,2").unwrap();
        let t = move_to_trash(&d, &s.trash, &s.home, now()).unwrap();
        assert!(!d.exists());
        restore(&t).unwrap();
        assert_eq!(fs::read(d.join("a/b/x.csv")).unwrap(), b"1,2");
    }

    #[test]
    fn same_names_get_different_places_in_the_trash_and_each_undo_is_exact() {
        let s = setup();
        let mut items = Vec::new();
        for i in 0..3u8 {
            let f = s.home.join(format!("d{i}/same.txt"));
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(&f, [i]).unwrap();
            items.push((f.clone(), move_to_trash(&f, &s.trash, &s.home, now()).unwrap()));
        }
        let mut places: Vec<_> = items.iter().map(|(_, t)| t.in_trash.clone()).collect();
        places.sort();
        places.dedup();
        assert_eq!(places.len(), 3);
        for (i, (orig, t)) in items.iter().enumerate().rev() {
            restore(t).unwrap();
            assert_eq!(fs::read(orig).unwrap(), [i as u8]);
        }
    }

    #[test]
    fn a_symlink_moves_itself_not_its_target() {
        let s = setup();
        let target = s.home.join("real.txt");
        fs::write(&target, b"keep").unwrap();
        let link = s.home.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let t = move_to_trash(&link, &s.trash, &s.home, now()).unwrap();
        assert!(fs::symlink_metadata(&link).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"keep");
        assert!(fs::symlink_metadata(&t.in_trash).unwrap().file_type().is_symlink());
        restore(&t).unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    }

    #[test]
    fn a_file_on_another_drive_is_refused_and_left_alone() {
        let s = setup();
        let tmp = tempfile::tempdir().unwrap();
        let disk_dev = fs::metadata(&s.home).unwrap().dev();
        if fs::metadata(tmp.path()).unwrap().dev() == disk_dev {
            return; // /tmp is on the same drive here: nothing to prove
        }
        let f = tmp.path().join("other.txt");
        fs::write(&f, b"x").unwrap();
        let e = move_to_trash(&f, &s.trash, &s.home, now()).unwrap_err();
        assert!(matches!(e, Error::OtherDrive(_)), "{e}");
        assert_eq!(fs::read(&f).unwrap(), b"x", "never deleted");
        let leftovers = fs::read_dir(s.trash.join("info")).map_or(0, Iterator::count);
        assert_eq!(leftovers, 0, "no half-written info file");
    }

    #[test]
    fn undo_never_overwrites() {
        let s = setup();
        let f = s.home.join("a.txt");
        fs::write(&f, b"old").unwrap();
        let t = move_to_trash(&f, &s.trash, &s.home, now()).unwrap();
        fs::write(&f, b"new").unwrap();
        assert!(matches!(restore(&t).unwrap_err(), Error::Occupied(_)));
        assert_eq!(fs::read(&f).unwrap(), b"new");
        assert!(t.in_trash.exists(), "still in the Trash");
    }

    #[test]
    fn hostile_paths_are_refused() {
        let s = setup();
        let try_it = |p: &Path| move_to_trash(p, &s.trash, &s.home, now());
        assert!(matches!(try_it(Path::new("relative/a")), Err(Error::NotAbsolute(_))));
        assert!(matches!(try_it(Path::new("")), Err(Error::NotAbsolute(_))));
        assert!(matches!(try_it(Path::new("/")), Err(Error::Protected(_))));
        assert!(matches!(try_it(&s.home), Err(Error::Protected(_))));
        assert!(matches!(try_it(&s.home.join("..")), Err(Error::Protected(_) | Error::NotFound(_))));
        assert!(matches!(try_it(&s.home.join("nope")), Err(Error::NotFound(_))));
        fs::create_dir_all(s.trash.join("files")).unwrap();
        assert!(matches!(try_it(&s.trash), Err(Error::Protected(_))));
        assert!(matches!(try_it(&s.trash.join("files")), Err(Error::Protected(_))));
        assert!(s.home.exists());
    }

    #[test]
    fn odd_names_survive_the_round_trip() {
        let s = setup();
        for name in ["with space.txt", "100%.txt", "new\nline", "ünï.txt", "a#b?c"] {
            let f = s.home.join(name);
            fs::write(&f, name).unwrap();
            let t = move_to_trash(&f, &s.trash, &s.home, now()).unwrap();
            let info = fs::read_to_string(&t.info).unwrap();
            assert_eq!(info.lines().count(), 3, "one line per key even for {name:?}: {info}");
            restore(&t).unwrap();
            assert_eq!(fs::read(&f).unwrap(), name.as_bytes());
        }
    }

    #[test]
    fn the_trash_folders_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let s = setup();
        let f = s.home.join("a");
        fs::write(&f, b"1").unwrap();
        move_to_trash(&f, &s.trash, &s.home, now()).unwrap();
        for d in [&s.trash, &s.trash.join("files"), &s.trash.join("info")] {
            assert_eq!(fs::metadata(d).unwrap().permissions().mode() & 0o777, 0o700, "{d:?}");
        }
    }

    #[test]
    fn the_default_trash_follows_xdg_data_home() {
        // Only checks the shape; the environment is not changed.
        if let Some(p) = default_trash_dir() {
            assert!(p.ends_with("Trash") && p.is_absolute());
        }
    }
}
