//! The user's settings, one small JSON file in `$XDG_CONFIG_HOME/tidewise/settings.json`. Saved as
//! soon as something changes. A missing file means defaults; a broken file is moved aside (never
//! deleted) and defaults are used, with a plain sentence for the screen. Nothing here leaves the
//! machine.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bumped when a field changes meaning. Readers ignore fields they do not know.
pub const VERSION: u32 = 1;

/// A settings file bigger than this is not ours: it is treated as broken.
pub const MAX_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    Dark,
    Light,
    /// Follow the desktop's light or dark mode. Also what an unknown value in the file means.
    #[serde(other)]
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub theme: ThemeChoice,
    /// The user's choice for the daily update check. Tidewise has no update check yet: this only
    /// remembers the answer.
    pub update_check: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: VERSION,
            theme: ThemeChoice::System,
            update_check: true,
        }
    }
}

/// What `load` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub settings: Settings,
    /// A plain sentence when the file was broken and defaults are used instead.
    pub problem: Option<String>,
}

/// `$XDG_CONFIG_HOME/tidewise`, else `$HOME/.config/tidewise`.
pub fn settings_dir_from(
    _xdg_config: Option<OsString>,
    _home: Option<OsString>,
) -> Option<PathBuf> {
    None
}

/// The settings folder for this user, from the environment.
pub fn settings_dir() -> Option<PathBuf> {
    settings_dir_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

pub fn load(_dir: &Path) -> Loaded {
    Loaded {
        settings: Settings::default(),
        problem: None,
    }
}

/// Write `settings.json` atomically (temp file, then rename), creating the folder.
pub fn save(_dir: &Path, _settings: &Settings) -> io::Result<()> {
    Err(io::Error::other("not built"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn the_defaults_follow_the_system_and_remember_an_update_check_choice() {
        let d = Settings::default();
        assert_eq!(
            (d.version, d.theme, d.update_check),
            (1, ThemeChoice::System, true)
        );
    }

    #[test]
    fn settings_live_under_the_config_folder() {
        let dir = |x: Option<&str>, h: Option<&str>| {
            settings_dir_from(x.map(Into::into), h.map(Into::into))
        };
        assert_eq!(dir(Some("/c"), Some("/h")), Some("/c/tidewise".into()));
        assert_eq!(dir(None, Some("/h")), Some("/h/.config/tidewise".into()));
        assert_eq!(
            dir(Some("relative"), Some("/h")),
            Some("/h/.config/tidewise".into())
        );
        assert_eq!(dir(Some(""), Some("")), None);
        assert_eq!(dir(None, None), None);
    }

    #[test]
    fn a_missing_file_means_defaults_and_nothing_is_created() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("tidewise");
        let l = load(&dir);
        assert_eq!((l.settings, l.problem), (Settings::default(), None));
        assert!(!dir.exists(), "reading never creates anything");
    }

    #[test]
    fn what_is_saved_is_read_back_and_the_file_is_plain_json() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("cfg/tidewise");
        let s = Settings {
            theme: ThemeChoice::Light,
            update_check: false,
            ..Settings::default()
        };
        save(&dir, &s).unwrap();
        assert_eq!(
            load(&dir),
            Loaded {
                settings: s.clone(),
                problem: None
            }
        );
        let text = fs::read_to_string(dir.join("settings.json")).unwrap();
        for needle in [
            "\"version\": 1",
            "\"theme\": \"light\"",
            "\"update_check\": false",
        ] {
            assert!(text.contains(needle), "{needle} in {text}");
        }
        let leftovers: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(leftovers.len(), 1, "no temp file stays behind");
        // Saving again replaces the file.
        save(&dir, &Settings::default()).unwrap();
        assert_eq!(load(&dir).settings, Settings::default());
    }

    #[test]
    fn a_broken_file_is_moved_aside_never_deleted_and_defaults_are_used() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        fs::write(dir.join("settings.json"), "{ this is not json").unwrap();
        let l = load(&dir);
        assert_eq!(l.settings, Settings::default());
        let why = l.problem.expect("a plain sentence");
        assert!(why.contains("settings.json.broken"), "{why}");
        assert!(!dir.join("settings.json").exists());
        assert_eq!(
            fs::read_to_string(dir.join("settings.json.broken")).unwrap(),
            "{ this is not json"
        );
        // The next load is clean.
        assert_eq!(load(&dir).problem, None);
    }

    #[test]
    fn wrong_types_and_huge_files_count_as_broken() {
        for (i, text) in [
            r#"{"update_check": "yes"}"#.to_string(),
            r#"[1,2]"#.into(),
            "x".repeat(70_000),
        ]
        .into_iter()
        .enumerate()
        {
            let tmp = tempfile::tempdir().unwrap();
            fs::write(tmp.path().join("settings.json"), &text).unwrap();
            let l = load(tmp.path());
            assert!(l.problem.is_some(), "case {i}");
            assert_eq!(l.settings, Settings::default());
        }
    }

    #[test]
    fn unknown_themes_and_fields_and_missing_fields_are_fine() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("settings.json"),
            r#"{"version": 9, "theme": "sepia", "future": {"x": 1}}"#,
        )
        .unwrap();
        let l = load(tmp.path());
        assert_eq!(l.problem, None);
        assert_eq!(
            l.settings.theme,
            ThemeChoice::System,
            "unknown value means follow the system"
        );
        assert!(l.settings.update_check, "missing field: its default");
        fs::write(tmp.path().join("settings.json"), "{}").unwrap();
        assert_eq!(
            load(tmp.path()),
            Loaded {
                settings: Settings::default(),
                problem: None
            }
        );
    }

    #[test]
    fn saving_into_a_place_that_cannot_be_written_is_a_plain_error() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a-file");
        fs::write(&file, b"x").unwrap();
        assert!(save(&file.join("tidewise"), &Settings::default()).is_err());
    }
}
