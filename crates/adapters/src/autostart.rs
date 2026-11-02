//! Startup apps: what starts when the user logs in, from the XDG autostart folders
//! (`/etc/xdg/autostart` and `~/.config/autostart`). Turning one off never edits a system file: it
//! writes a user copy with `Hidden=true` (and keeps a backup of a user file it edits). Turning it
//! back on undoes exactly that. Parts of the desktop session, audio and network are protected:
//! listed, but refused in code.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What one `.desktop` autostart file says (the `[Desktop Entry]` group only; localized names are
/// ignored).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    pub name: String,
    pub comment: Option<String>,
    pub exec: String,
    pub icon: Option<String>,
    /// `Hidden=true`: as if the file did not exist.
    pub hidden: bool,
    /// `X-GNOME-Autostart-enabled=false`.
    pub gnome_disabled: bool,
    pub only_show_in: Vec<String>,
    pub not_show_in: Vec<String>,
    /// Written by Tidewise when it made a user copy of a system file (`X-Tidewise-Created=true`).
    pub tidewise_created: bool,
}

/// Read one file. `None` when it is not a `[Desktop Entry]` of type Application without an `Exec`.
pub fn parse(_text: &str) -> Option<Parsed> {
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The effective file is in a system folder.
    System,
    /// The effective file is in the user's autostart folder.
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupEntry {
    /// The file name, e.g. `discord.desktop`.
    pub file: String,
    pub name: String,
    pub comment: Option<String>,
    pub exec: String,
    pub icon: Option<String>,
    /// The effective file.
    pub path: PathBuf,
    pub scope: Scope,
    /// Starts at login right now.
    pub enabled: bool,
    /// Part of the desktop session, audio or network: no switch.
    pub protected: bool,
    /// A file of the same name exists in a system folder too.
    pub has_system_copy: bool,
}

/// Parts of the session that must keep starting (golden rule 7): the desktop shell, the
/// authentication agent, the keyring and wallet, accessibility bus, audio, portals and the network
/// applet. Matched on the file name, case-insensitively.
pub fn is_protected(_file: &str) -> bool {
    false
}

/// Every startup entry that applies to `desktop` (`XDG_CURRENT_DESKTOP`, e.g. `KDE` or
/// `ubuntu:GNOME`; `None` shows all), sorted by name. A user file hides the system file of the same
/// name; system files that are `Hidden=true` are left out.
pub fn list(
    _system_dirs: &[PathBuf],
    _user_dir: &Path,
    _desktop: Option<&str>,
) -> Vec<StartupEntry> {
    Vec::new()
}

/// Where things live, so tests can use temp folders.
pub struct Dirs<'a> {
    pub system: &'a [PathBuf],
    pub user: &'a Path,
    /// Backups of user files Tidewise edits.
    pub backup: &'a Path,
    pub trash: &'a Path,
    pub home: &'a Path,
    pub now: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("`{0}` is not a plain startup file name")]
    BadName(String),
    #[error("{0} is part of your desktop session, so Tidewise leaves it alone")]
    Protected(String),
    #[error("{0} was not found in the startup folders")]
    NotFound(String),
    #[error("{0}")]
    Io(String),
}

/// Turn an entry off: a user copy with `Hidden=true`.
pub fn disable(_file: &str, _dirs: &Dirs) -> Result<(), Error> {
    Err(Error::Io("not built".into()))
}

/// Turn an entry back on, undoing what `disable` did.
pub fn enable(_file: &str, _dirs: &Dirs) -> Result<(), Error> {
    Err(Error::Io("not built".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fx(name: &str) -> String {
        fs::read_to_string(format!(
            "{}/../../tests/fixtures/autostart/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    #[test]
    fn real_system_files_give_name_exec_and_where_they_apply() {
        let b = parse(&fx("baloo_file.desktop")).unwrap();
        assert_eq!(b.name, "Baloo File Daemon");
        assert_eq!(b.exec, "/usr/lib/kf6/baloo_file");
        assert!(!b.hidden && !b.gnome_disabled && !b.tidewise_created);
        assert!(b.only_show_in.iter().any(|d| d == "KDE"));
        let t = parse(&fx("localsearch-3.desktop")).unwrap();
        assert_eq!(t.name, "Tracker File System Miner");
        assert_eq!(
            t.comment.as_deref(),
            Some("Crawls and processes files on the file system")
        );
        assert_eq!(
            parse(&fx("org.kde.discover.notifier.desktop"))
                .unwrap()
                .name,
            "Discover"
        );
    }

    #[test]
    fn a_user_file_with_localized_names_and_actions_reads_the_plain_entry_only() {
        let d = parse(&fx("discord-handwritten.desktop")).unwrap();
        assert_eq!(d.name, "Discord");
        assert_eq!(d.comment.as_deref(), Some("All-in-one voice and text chat"));
        assert_eq!(d.exec, "/usr/bin/discord --start-minimized");
        assert_eq!(d.icon.as_deref(), Some("discord"));
    }

    #[test]
    fn disabled_hidden_and_broken_files() {
        assert!(
            parse(&fx("disabled-gnome-handwritten.desktop"))
                .unwrap()
                .gnome_disabled
        );
        assert!(parse(&fx("hidden-handwritten.desktop")).unwrap().hidden);
        assert_eq!(parse(&fx("broken-handwritten.desktop")), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("[Desktop Entry]\nType=Link\nName=x\nExec=y\n"), None);
        assert_eq!(
            parse("[Desktop Entry]\nType=Application\nName=x\n"),
            None,
            "no Exec"
        );
    }

    #[test]
    fn session_essentials_are_protected_and_ordinary_apps_are_not() {
        for p in [
            "org.kde.plasmashell.desktop",
            "polkit-kde-authentication-agent-1.desktop",
            "polkit-gnome-authentication-agent-1.desktop",
            "nm-applet.desktop",
            "pipewire.desktop",
            "wireplumber.desktop",
            "xdg-desktop-portal.desktop",
            "gnome-keyring-secrets.desktop",
            "pam_kwallet_init.desktop",
            "kglobalacceld.desktop",
            "at-spi-dbus-bus.desktop",
            "ORG.KDE.PLASMASHELL.DESKTOP",
        ] {
            assert!(is_protected(p), "{p}");
        }
        for p in [
            "discord.desktop",
            "baloo_file.desktop",
            "spotify.desktop",
            "org.kde.discover.notifier.desktop",
        ] {
            assert!(!is_protected(p), "{p}");
        }
    }

    struct World {
        _tmp: tempfile::TempDir,
        system: Vec<PathBuf>,
        user: PathBuf,
        backup: PathBuf,
        trash: PathBuf,
        home: PathBuf,
    }

    fn world() -> World {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let sys = tmp.path().join("etc/xdg/autostart");
        let user = home.join(".config/autostart");
        fs::create_dir_all(&sys).unwrap();
        fs::create_dir_all(&user).unwrap();
        for f in [
            "baloo_file.desktop",
            "localsearch-3.desktop",
            "xapp-sn-watcher.desktop",
            "org.kde.plasmashell.desktop",
        ] {
            fs::write(sys.join(f), fx(f)).unwrap();
        }
        fs::write(
            sys.join("not-this-desktop.desktop"),
            fx("not-this-desktop-handwritten.desktop"),
        )
        .unwrap();
        fs::write(sys.join("hidden.desktop"), fx("hidden-handwritten.desktop")).unwrap();
        fs::write(
            sys.join("old-sync.desktop"),
            fx("disabled-gnome-handwritten.desktop"),
        )
        .unwrap();
        fs::write(
            user.join("discord.desktop"),
            fx("discord-handwritten.desktop"),
        )
        .unwrap();
        World {
            system: vec![sys],
            backup: home.join(".local/state/tidewise/backup"),
            trash: home.join(".local/share/Trash"),
            user,
            home,
            _tmp: tmp,
        }
    }

    impl World {
        fn dirs(&self) -> Dirs<'_> {
            Dirs {
                system: &self.system,
                user: &self.user,
                backup: &self.backup,
                trash: &self.trash,
                home: &self.home,
                now: SystemTime::UNIX_EPOCH,
            }
        }
        fn list(&self, desktop: Option<&str>) -> Vec<StartupEntry> {
            list(&self.system, &self.user, desktop)
        }
        fn entry(&self, file: &str) -> StartupEntry {
            self.list(None)
                .into_iter()
                .find(|e| e.file == file)
                .unwrap()
        }
    }

    #[test]
    fn the_list_is_sorted_by_name_and_only_has_what_applies_to_this_desktop() {
        let w = world();
        let names = |d| w.list(d).into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(
            names(Some("KDE")),
            [
                "Baloo File Daemon",
                "Discord",
                "Old Sync Tool",
                "Plasma",
                "Tracker File System Miner",
                "xapp-sn-watcher"
            ]
        );
        let gnome = names(Some("ubuntu:GNOME"));
        assert!(gnome.contains(&"GNOME Only".to_string()), "{gnome:?}");
        assert!(
            names(None).contains(&"GNOME Only".to_string()),
            "unknown desktop shows all"
        );
        assert!(
            !names(None).contains(&"Hidden By Hand".to_string()),
            "hidden system files are left out"
        );
    }

    #[test]
    fn each_entry_says_where_it_lives_whether_it_is_on_and_whether_it_is_protected() {
        let w = world();
        let d = w.entry("discord.desktop");
        assert_eq!(
            (d.scope, d.enabled, d.protected, d.has_system_copy),
            (Scope::User, true, false, false)
        );
        assert_eq!(d.path, w.user.join("discord.desktop"));
        let p = w.entry("org.kde.plasmashell.desktop");
        assert_eq!((p.scope, p.protected), (Scope::System, true));
        let o = w.entry("old-sync.desktop");
        assert!(
            !o.enabled,
            "X-GNOME-Autostart-enabled=false means it does not start"
        );
    }

    #[test]
    fn a_user_file_hides_the_system_file_of_the_same_name() {
        let w = world();
        let mut text = fx("baloo_file.desktop");
        text.push_str("Hidden=true\n");
        fs::write(w.user.join("baloo_file.desktop"), text).unwrap();
        let b = w.entry("baloo_file.desktop");
        assert_eq!(
            (b.scope, b.enabled, b.has_system_copy),
            (Scope::User, false, true)
        );
        assert_eq!(
            w.list(None)
                .iter()
                .filter(|e| e.file == "baloo_file.desktop")
                .count(),
            1
        );
    }

    #[test]
    fn turning_off_a_system_entry_writes_a_user_copy_and_never_touches_the_system_file() {
        let w = world();
        let sys_file = w.system[0].join("baloo_file.desktop");
        let before = fs::read(&sys_file).unwrap();
        disable("baloo_file.desktop", &w.dirs()).unwrap();
        assert_eq!(fs::read(&sys_file).unwrap(), before);
        let copy = fs::read_to_string(w.user.join("baloo_file.desktop")).unwrap();
        let p = parse(&copy).unwrap();
        assert!(p.hidden && p.tidewise_created, "{copy}");
        assert_eq!(p.name, "Baloo File Daemon");
        let e = w.entry("baloo_file.desktop");
        assert_eq!((e.scope, e.enabled), (Scope::User, false));
        // Every original line is still there.
        for line in fx("baloo_file.desktop").lines().filter(|l| !l.is_empty()) {
            assert!(copy.lines().any(|c| c == line), "lost {line}");
        }
    }

    #[test]
    fn turning_off_a_user_entry_keeps_a_backup_once_and_all_other_lines() {
        let w = world();
        let original = fx("discord-handwritten.desktop");
        disable("discord.desktop", &w.dirs()).unwrap();
        assert_eq!(
            fs::read_to_string(w.backup.join("discord.desktop")).unwrap(),
            original
        );
        let now = fs::read_to_string(w.user.join("discord.desktop")).unwrap();
        assert!(parse(&now).unwrap().hidden);
        assert!(
            now.contains("# Written by the app itself") && now.contains("[Desktop Action Quit]"),
            "{now}"
        );
        // A second round does not overwrite the backup with the edited file.
        enable("discord.desktop", &w.dirs()).unwrap();
        disable("discord.desktop", &w.dirs()).unwrap();
        assert_eq!(
            fs::read_to_string(w.backup.join("discord.desktop")).unwrap(),
            original
        );
    }

    #[test]
    fn turning_a_user_entry_back_on_restores_the_original_file_exactly() {
        let w = world();
        let original = fs::read(w.user.join("discord.desktop")).unwrap();
        disable("discord.desktop", &w.dirs()).unwrap();
        enable("discord.desktop", &w.dirs()).unwrap();
        assert_eq!(fs::read(w.user.join("discord.desktop")).unwrap(), original);
        assert!(
            !w.backup.join("discord.desktop").exists(),
            "the used backup is gone"
        );
        assert!(w.entry("discord.desktop").enabled);
    }

    #[test]
    fn turning_a_system_entry_back_on_moves_our_copy_to_the_trash() {
        let w = world();
        disable("baloo_file.desktop", &w.dirs()).unwrap();
        enable("baloo_file.desktop", &w.dirs()).unwrap();
        assert!(!w.user.join("baloo_file.desktop").exists());
        assert!(
            w.trash.join("files/baloo_file.desktop").exists(),
            "never deleted, always the Trash"
        );
        let e = w.entry("baloo_file.desktop");
        assert_eq!((e.scope, e.enabled), (Scope::System, true));
    }

    #[test]
    fn a_hand_hidden_user_file_without_a_backup_is_switched_on_by_flipping_the_flag() {
        let w = world();
        fs::write(
            w.user.join("by-hand.desktop"),
            fx("hidden-handwritten.desktop"),
        )
        .unwrap();
        enable("by-hand.desktop", &w.dirs()).unwrap();
        let p = parse(&fs::read_to_string(w.user.join("by-hand.desktop")).unwrap()).unwrap();
        assert!(!p.hidden);
        assert_eq!(p.name, "Hidden By Hand");
    }

    #[test]
    fn a_system_entry_that_gnome_disabled_can_be_switched_on_with_a_user_copy() {
        let w = world();
        enable("old-sync.desktop", &w.dirs()).unwrap();
        let p = parse(&fs::read_to_string(w.user.join("old-sync.desktop")).unwrap()).unwrap();
        assert!(!p.hidden && !p.gnome_disabled && p.tidewise_created);
        assert!(w.entry("old-sync.desktop").enabled);
    }

    #[test]
    fn names_that_could_leave_the_folder_are_refused_and_nothing_is_written() {
        let w = world();
        let outside = w.home.join("evil.desktop");
        for bad in [
            "../evil.desktop",
            "a/b.desktop",
            ".hidden.desktop",
            "evil",
            "",
            "x\0.desktop",
            "..",
            "-rf.desktop",
        ] {
            assert!(
                matches!(disable(bad, &w.dirs()), Err(Error::BadName(_))),
                "{bad:?}"
            );
            assert!(
                matches!(enable(bad, &w.dirs()), Err(Error::BadName(_))),
                "{bad:?}"
            );
        }
        let long = format!("{}.desktop", "a".repeat(300));
        assert!(matches!(disable(&long, &w.dirs()), Err(Error::BadName(_))));
        assert!(!outside.exists());
        assert_eq!(
            fs::read_dir(&w.user).unwrap().count(),
            1,
            "only discord.desktop"
        );
    }

    #[test]
    fn protected_and_unknown_entries_are_refused_by_name() {
        let w = world();
        assert!(matches!(
            disable("org.kde.plasmashell.desktop", &w.dirs()),
            Err(Error::Protected(_))
        ));
        assert!(matches!(
            enable("org.kde.plasmashell.desktop", &w.dirs()),
            Err(Error::Protected(_))
        ));
        assert!(matches!(
            disable("nope.desktop", &w.dirs()),
            Err(Error::NotFound(_))
        ));
        assert!(!w.user.join("org.kde.plasmashell.desktop").exists());
    }

    #[test]
    fn no_temporary_files_stay_behind() {
        let w = world();
        disable("baloo_file.desktop", &w.dirs()).unwrap();
        disable("discord.desktop", &w.dirs()).unwrap();
        let names: Vec<_> = fs::read_dir(&w.user)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| n.ends_with(".desktop")), "{names:?}");
    }
}
