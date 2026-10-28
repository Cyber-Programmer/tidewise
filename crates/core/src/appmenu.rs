//! "Add to app menu": writes the launcher and icon into the user's own data folder
//! (`$XDG_DATA_HOME`, usually `~/.local/share`). Nothing outside it is touched, and removing the
//! entry deletes exactly the two files this module wrote.

use std::io;
use std::path::{Path, PathBuf};

const DESKTOP_TEMPLATE: &str =
    include_str!("../../../packaging/appimage/io.github.tidewise.desktop");
const ICON: &str = include_str!("../../../docs/design/icons/logo.svg");

/// Where the launcher and the icon go under `data_home`.
pub fn paths(data_home: &Path) -> (PathBuf, PathBuf) {
    (
        data_home.join("applications/io.github.tidewise.desktop"),
        data_home.join("icons/hicolor/scalable/apps/io.github.tidewise.svg"),
    )
}

/// The launcher text with `Exec=` pointing at `exe` (quoted, so spaces in the path work).
pub fn desktop_entry(exe: &Path) -> String {
    // Desktop Entry spec: inside double quotes escape " ` $ \ with a backslash; `%` becomes `%%`.
    let mut quoted = String::from("\"");
    for c in exe.to_string_lossy().chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    DESKTOP_TEMPLATE
        .lines()
        .map(|l| {
            if l.starts_with("Exec=") {
                format!("Exec={quoted}\n")
            } else {
                format!("{l}\n")
            }
        })
        .collect()
}

/// Is the entry installed?
pub fn is_installed(data_home: &Path) -> bool {
    let (desktop, icon) = paths(data_home);
    desktop.is_file() && icon.is_file()
}

/// Write the launcher and icon. Safe to run again: the files are replaced with the same content.
pub fn install(data_home: &Path, exe: &Path) -> io::Result<()> {
    if !exe.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the program's location must be a full path",
        ));
    }
    let (desktop, icon) = paths(data_home);
    for (path, text) in [(&icon, ICON.to_string()), (&desktop, desktop_entry(exe))] {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write beside it and rename, so a launcher never sees half a file.
        let tmp = path.with_extension("tmp-tidewise");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
    }
    Ok(())
}

/// Remove what `install` wrote. Fine if it is already gone.
pub fn remove(data_home: &Path) -> io::Result<()> {
    let (desktop, icon) = paths(data_home);
    for path in [desktop, icon] {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// `$XDG_DATA_HOME`, or `~/.local/share`.
pub fn data_home() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn exe() -> PathBuf {
        PathBuf::from("/home/ali/Apps/Tidewise 0.1.AppImage")
    }

    #[test]
    fn the_entry_points_at_the_exe_and_keeps_the_rest_of_the_template() {
        let text = desktop_entry(&exe());
        assert!(
            text.contains("\nExec=\"/home/ali/Apps/Tidewise 0.1.AppImage\"\n"),
            "{text}"
        );
        assert!(text.contains("Name=Tidewise") && text.contains("Icon=io.github.tidewise"));
        assert_eq!(text.matches("Exec=").count(), 1);
    }

    #[test]
    fn quotes_and_percent_in_a_path_are_escaped() {
        let text = desktop_entry(Path::new("/odd/\"q\" 100%/tidewise"));
        assert!(
            text.contains(r#"Exec="/odd/\"q\" 100%%/tidewise""#),
            "{text}"
        );
    }

    #[test]
    fn install_writes_both_files_and_remove_deletes_only_those() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        fs::create_dir_all(home.join("applications")).unwrap();
        fs::write(home.join("applications/other.desktop"), "keep").unwrap();
        assert!(!is_installed(home));
        install(home, &exe()).unwrap();
        let (desktop, icon) = paths(home);
        assert_eq!(
            desktop,
            home.join("applications/io.github.tidewise.desktop")
        );
        assert_eq!(
            icon,
            home.join("icons/hicolor/scalable/apps/io.github.tidewise.svg")
        );
        assert!(fs::read_to_string(&desktop).unwrap().contains("Exec="));
        assert!(fs::read_to_string(&icon).unwrap().starts_with("<svg"));
        assert!(is_installed(home));
        remove(home).unwrap();
        assert!(!desktop.exists() && !icon.exists());
        assert_eq!(
            fs::read_to_string(home.join("applications/other.desktop")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn install_twice_is_the_same_as_once_and_updates_the_path() {
        let dir = tempfile::tempdir().unwrap();
        install(dir.path(), &exe()).unwrap();
        let first = fs::read(paths(dir.path()).0).unwrap();
        install(dir.path(), &exe()).unwrap();
        assert_eq!(fs::read(paths(dir.path()).0).unwrap(), first);
        install(dir.path(), Path::new("/opt/tidewise")).unwrap();
        assert!(fs::read_to_string(paths(dir.path()).0)
            .unwrap()
            .contains("Exec=\"/opt/tidewise\""));
    }

    #[test]
    fn remove_when_nothing_is_installed_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        remove(dir.path()).unwrap();
        remove(&dir.path().join("missing")).unwrap();
    }

    #[test]
    fn a_relative_exe_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let e = install(dir.path(), Path::new("tidewise")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        assert!(!is_installed(dir.path()));
    }
}
