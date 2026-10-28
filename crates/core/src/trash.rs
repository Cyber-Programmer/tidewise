//! Move to Trash and Undo, following the freedesktop Trash spec for the user's own Trash
//! (`$XDG_DATA_HOME/Trash` with `files/` and `info/`). Never deletes: a file that cannot be moved
//! (another filesystem, no permission) stays where it is and the error says so.
//!
//! Not done on purpose: the spec's per-drive `.Trash-<uid>` folders. They would write on drives the
//! user did not pick, so a file on another drive is refused with [`Error::OtherDrive`].

use std::ffi::OsString;
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
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
    path: &Path,
    trash_dir: &Path,
    home: &Path,
    now: SystemTime,
) -> Result<Trashed, Error> {
    if !path.is_absolute() {
        return Err(Error::NotAbsolute(path.to_path_buf()));
    }
    let protected = path.components().any(|c| c == Component::ParentDir)
        || home.starts_with(path)
        || path.starts_with(trash_dir)
        || trash_dir.starts_with(path);
    if protected {
        return Err(Error::Protected(path.to_path_buf()));
    }
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Error::Io { path, source }
    };
    let meta = fs::symlink_metadata(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => Error::NotFound(path.to_path_buf()),
        _ => io(path)(e),
    })?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::Protected(path.to_path_buf()))?
        .as_bytes()
        .to_vec();
    let (files, infos) = (trash_dir.join("files"), trash_dir.join("info"));
    for dir in [&files, &infos] {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(io(dir))?;
    }
    for dir in [trash_dir, &files, &infos] {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(io(dir))?;
    }
    let trash_device = fs::metadata(&files).map_err(io(&files))?.dev();
    if meta.dev() != trash_device {
        return Err(Error::OtherDrive(path.to_path_buf()));
    }
    let text = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_encode(path.as_os_str().as_bytes()),
        deletion_date(now)
    );
    for n in 1..10_000u32 {
        let mut candidate = name.clone();
        if n > 1 {
            candidate.extend_from_slice(format!(".{n}").as_bytes());
        }
        let candidate = OsString::from_vec(candidate);
        let mut info_name = candidate.clone();
        info_name.push(".trashinfo");
        let (info, target) = (infos.join(info_name), files.join(&candidate));
        // Reserving the info file first makes the name ours even if two moves run at once.
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info)
        {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io(&info)(e)),
        };
        let undo = |e: Error| {
            let _ = fs::remove_file(&info);
            e
        };
        if fs::symlink_metadata(&target).is_ok() {
            let _ = undo(Error::Occupied(target));
            continue;
        }
        file.write_all(text.as_bytes())
            .map_err(|e| undo(io(&info)(e)))?;
        return match fs::rename(path, &target) {
            Ok(()) => Ok(Trashed {
                original: path.to_path_buf(),
                in_trash: target,
                info,
            }),
            Err(e) if e.raw_os_error() == Some(EXDEV) => {
                Err(undo(Error::OtherDrive(path.to_path_buf())))
            }
            Err(e) => Err(undo(io(path)(e))),
        };
    }
    Err(Error::Occupied(trash_dir.to_path_buf()))
}

/// Put a trashed item back exactly where it was. Refuses to overwrite anything.
pub fn restore(item: &Trashed) -> Result<(), Error> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Error::Io { path, source }
    };
    if fs::symlink_metadata(&item.in_trash).is_err() {
        return Err(Error::NotFound(item.in_trash.clone()));
    }
    if fs::symlink_metadata(&item.original).is_ok() {
        return Err(Error::Occupied(item.original.clone()));
    }
    if let Some(parent) = item.original.parent() {
        fs::create_dir_all(parent).map_err(io(parent))?;
    }
    match fs::rename(&item.in_trash, &item.original) {
        Ok(()) => {}
        Err(e) if e.raw_os_error() == Some(EXDEV) => {
            return Err(Error::OtherDrive(item.original.clone()))
        }
        Err(e) => return Err(io(&item.original)(e)),
    }
    let _ = fs::remove_file(&item.info);
    Ok(())
}

/// `errno` for "cross-device link".
const EXDEV: i32 = 18;

/// Percent-encode as the spec asks: everything except `A-Za-z0-9-_.~/`.
fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `YYYY-MM-DDThh:mm:ss`. The spec asks for local time; without a time-zone library this is UTC.
fn deletion_date(now: SystemTime) -> String {
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Temp folders on the project's disk (not tmpfs), like `scan`'s tests.
    fn disk_dir() -> tempfile::TempDir {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp");
        fs::create_dir_all(&base).unwrap();
        // Paths with `..` in them are refused on purpose, so the fixture path must be clean.
        tempfile::Builder::new()
            .tempdir_in(base.canonicalize().unwrap())
            .unwrap()
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
        assert!(
            info.contains("a.iso") && info.contains("DeletionDate=2025-10-28T"),
            "{info}"
        );
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
            items.push((
                f.clone(),
                move_to_trash(&f, &s.trash, &s.home, now()).unwrap(),
            ));
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
        assert!(fs::symlink_metadata(&t.in_trash)
            .unwrap()
            .file_type()
            .is_symlink());
        restore(&t).unwrap();
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
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
        assert!(matches!(
            try_it(Path::new("relative/a")),
            Err(Error::NotAbsolute(_))
        ));
        assert!(matches!(try_it(Path::new("")), Err(Error::NotAbsolute(_))));
        assert!(matches!(try_it(Path::new("/")), Err(Error::Protected(_))));
        assert!(matches!(try_it(&s.home), Err(Error::Protected(_))));
        assert!(matches!(
            try_it(&s.home.join("..")),
            Err(Error::Protected(_) | Error::NotFound(_))
        ));
        assert!(matches!(
            try_it(&s.home.join("nope")),
            Err(Error::NotFound(_))
        ));
        fs::create_dir_all(s.trash.join("files")).unwrap();
        assert!(matches!(try_it(&s.trash), Err(Error::Protected(_))));
        assert!(matches!(
            try_it(&s.trash.join("files")),
            Err(Error::Protected(_))
        ));
        assert!(s.home.exists());
    }

    #[test]
    fn odd_names_survive_the_round_trip() {
        let s = setup();
        for name in [
            "with space.txt",
            "100%.txt",
            "new\nline",
            "ünï.txt",
            "a#b?c",
        ] {
            let f = s.home.join(name);
            fs::write(&f, name).unwrap();
            let t = move_to_trash(&f, &s.trash, &s.home, now()).unwrap();
            let info = fs::read_to_string(&t.info).unwrap();
            assert_eq!(
                info.lines().count(),
                3,
                "one line per key even for {name:?}: {info}"
            );
            restore(&t).unwrap();
            assert_eq!(fs::read(&f).unwrap(), name.as_bytes());
        }
    }

    #[test]
    fn the_trash_folders_are_private() {
        let s = setup();
        let f = s.home.join("a");
        fs::write(&f, b"1").unwrap();
        move_to_trash(&f, &s.trash, &s.home, now()).unwrap();
        for d in [&s.trash, &s.trash.join("files"), &s.trash.join("info")] {
            assert_eq!(
                fs::metadata(d).unwrap().permissions().mode() & 0o777,
                0o700,
                "{d:?}"
            );
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
