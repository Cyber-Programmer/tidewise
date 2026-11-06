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

/// Read one file. `None` when it is not a `[Desktop Entry]` of type Application with an `Exec`.
pub fn parse(text: &str) -> Option<Parsed> {
    let mut in_group = false;
    let mut found_group = false;
    let (mut kind, mut p) = (None, Parsed::default());
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            found_group |= in_group;
            continue;
        }
        if !in_group || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let list = |v: &str| {
            v.split(';')
                .filter(|d| !d.is_empty())
                .map(String::from)
                .collect::<Vec<_>>()
        };
        match key {
            "Type" => kind = Some(value.to_string()),
            "Name" => p.name = value.to_string(),
            "Comment" => p.comment = Some(value.to_string()).filter(|c| !c.is_empty()),
            "Exec" => p.exec = value.to_string(),
            "Icon" => p.icon = Some(value.to_string()).filter(|c| !c.is_empty()),
            "Hidden" => p.hidden = value == "true",
            "X-GNOME-Autostart-enabled" => p.gnome_disabled = value == "false",
            "OnlyShowIn" => p.only_show_in = list(value),
            "NotShowIn" => p.not_show_in = list(value),
            "X-Tidewise-Created" => p.tidewise_created = value == "true",
            _ => {}
        }
    }
    (found_group && kind.as_deref() == Some("Application") && !p.exec.is_empty()).then_some(p)
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
pub fn is_protected(file: &str) -> bool {
    const EXACT: [&str; 6] = [
        "org.kde.plasmashell",
        "kglobalacceld",
        "at-spi-dbus-bus",
        "pam_kwallet_init",
        "nm-applet",
        "pulseaudio",
    ];
    const PREFIX: [&str; 12] = [
        "polkit-",
        "gnome-keyring",
        "xdg-desktop-portal",
        "pipewire",
        "wireplumber",
        "gnome-shell",
        "gnome-session",
        "org.kde.plasma",
        "plasma-",
        "kwin",
        "kded",
        "ksmserver",
    ];
    let lower = file.to_lowercase();
    let stem = lower.strip_suffix(".desktop").unwrap_or(&lower);
    EXACT.contains(&stem) || PREFIX.iter().any(|p| stem.starts_with(p))
}

/// A plain file name for a startup entry: ends in `.desktop`, no separators, NUL or leading `.` or `-`.
fn is_plain_file(name: &str) -> bool {
    name.len() <= 255
        && name.len() > ".desktop".len()
        && name.ends_with(".desktop")
        && !name.starts_with(['.', '-'])
        && !name.contains(['/', '\\', '\0'])
}

fn read_small(path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    (meta.is_file() && meta.len() < 256 * 1024)
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
}

fn applies(p: &Parsed, desktop: Option<&str>) -> bool {
    let Some(current) = desktop else {
        return true;
    };
    let current: Vec<&str> = current
        .split([':', ';'])
        .filter(|d| !d.is_empty())
        .collect();
    let any = |list: &[String]| list.iter().any(|d| current.contains(&d.as_str()));
    (p.only_show_in.is_empty() || any(&p.only_show_in)) && !any(&p.not_show_in)
}

/// Every startup entry that applies to `desktop` (`XDG_CURRENT_DESKTOP`, e.g. `KDE` or
/// `ubuntu:GNOME`; `None` shows all), sorted by name. A user file hides the system file of the same
/// name; system files that are `Hidden=true` are left out.
pub fn list(system_dirs: &[PathBuf], user_dir: &Path, desktop: Option<&str>) -> Vec<StartupEntry> {
    use std::collections::{HashMap, HashSet};
    let files_in = |dir: &Path| -> Vec<(String, PathBuf)> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                is_plain_file(&name).then(|| (name, e.path()))
            })
            .collect()
    };
    // Earlier system folders win over later ones; the user folder wins over all.
    let mut chosen: HashMap<String, (PathBuf, Scope)> = HashMap::new();
    let mut in_system: HashSet<String> = HashSet::new();
    for dir in system_dirs {
        for (name, path) in files_in(dir) {
            in_system.insert(name.clone());
            chosen.entry(name).or_insert((path, Scope::System));
        }
    }
    for (name, path) in files_in(user_dir) {
        chosen.insert(name, (path, Scope::User));
    }
    let mut out: Vec<StartupEntry> = chosen
        .into_iter()
        .filter_map(|(file, (path, scope))| {
            let p = parse(&read_small(&path)?)?;
            if (scope == Scope::System && p.hidden) || !applies(&p, desktop) {
                return None;
            }
            let name = if p.name.is_empty() {
                file.trim_end_matches(".desktop").to_string()
            } else {
                p.name.clone()
            };
            Some(StartupEntry {
                protected: is_protected(&file),
                has_system_copy: in_system.contains(&file),
                enabled: !p.hidden && !p.gnome_disabled,
                comment: p.comment,
                exec: p.exec,
                icon: p.icon,
                file,
                name,
                path,
                scope,
            })
        })
        .collect();
    out.sort_by_key(|e| (e.name.to_lowercase(), e.file.clone()));
    out
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

/// Set `key=value` inside the `[Desktop Entry]` group: replace the line, or add it right after the
/// group header. Every other line (comments, other groups, localized names) stays as it is.
fn set_key(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let Some(header) = lines.iter().position(|l| l.trim() == "[Desktop Entry]") else {
        return text.to_string();
    };
    let end = lines[header + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| header + 1 + i);
    let wanted = format!("{key}={value}");
    match lines[header + 1..end]
        .iter()
        .position(|l| l.trim_start().starts_with(&format!("{key}=")))
    {
        Some(i) => lines[header + 1 + i] = wanted,
        None => lines.insert(header + 1, wanted),
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Write `text` to `path` through a temporary file in the same folder, then rename it.
fn write_atomic(path: &Path, text: &str) -> Result<(), Error> {
    let io = |e: std::io::Error| Error::Io(format!("{}: {e}", path.display()));
    let dir = path.parent().ok_or_else(|| Error::Io("no folder".into()))?;
    std::fs::create_dir_all(dir).map_err(io)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let temp = dir.join(format!(".{name}.tmp-tidewise"));
    let result = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(io)
}

fn check(file: &str) -> Result<(), Error> {
    if !is_plain_file(file) {
        return Err(Error::BadName(file.to_string()));
    }
    if is_protected(file) {
        return Err(Error::Protected(file.to_string()));
    }
    Ok(())
}

/// The first system folder's copy of `file`.
fn system_copy(file: &str, dirs: &Dirs) -> Option<String> {
    dirs.system.iter().find_map(|d| read_small(&d.join(file)))
}

/// Turn an entry off: a user copy with `Hidden=true`.
pub fn disable(file: &str, dirs: &Dirs) -> Result<(), Error> {
    check(file)?;
    let user_file = dirs.user.join(file);
    if let Some(text) = read_small(&user_file) {
        if parse(&text).is_none_or(|p| p.hidden) {
            return Ok(()); // already off (or not a startup file at all)
        }
        // The first time only: what the app wrote, so turning it on gives it back unchanged.
        let backup = dirs.backup.join(file);
        if !backup.exists() {
            std::fs::create_dir_all(dirs.backup).map_err(|e| Error::Io(e.to_string()))?;
            std::fs::write(&backup, &text).map_err(|e| Error::Io(e.to_string()))?;
        }
        return write_atomic(&user_file, &set_key(&text, "Hidden", "true"));
    }
    let text = system_copy(file, dirs).ok_or_else(|| Error::NotFound(file.to_string()))?;
    let copy = set_key(
        &set_key(&text, "Hidden", "true"),
        "X-Tidewise-Created",
        "true",
    );
    write_atomic(&user_file, &copy)
}

/// Turn an entry back on, undoing what `disable` did.
pub fn enable(file: &str, dirs: &Dirs) -> Result<(), Error> {
    check(file)?;
    let user_file = dirs.user.join(file);
    if let Some(text) = read_small(&user_file) {
        let parsed = parse(&text).ok_or_else(|| Error::NotFound(file.to_string()))?;
        if parsed.tidewise_created {
            // Our own copy of a system file: the Trash, never a delete; the system file runs again.
            return tidewise_core::trash::move_to_trash(
                &user_file, dirs.trash, dirs.home, dirs.now,
            )
            .map(|_| ())
            .map_err(|e| Error::Io(e.to_string()));
        }
        let backup = dirs.backup.join(file);
        if let Some(original) = read_small(&backup) {
            write_atomic(&user_file, &original)?;
            let _ = std::fs::remove_file(&backup); // our own backup, used up
            return Ok(());
        }
        let mut text = set_key(&text, "Hidden", "false");
        if parsed.gnome_disabled {
            text = set_key(&text, "X-GNOME-Autostart-enabled", "true");
        }
        return write_atomic(&user_file, &text);
    }
    let text = system_copy(file, dirs).ok_or_else(|| Error::NotFound(file.to_string()))?;
    if parse(&text).is_none_or(|p| !p.gnome_disabled) {
        return Ok(()); // already on
    }
    let copy = set_key(
        &set_key(&text, "Hidden", "false"),
        "X-GNOME-Autostart-enabled",
        "true",
    );
    write_atomic(&user_file, &set_key(&copy, "X-Tidewise-Created", "true"))
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
