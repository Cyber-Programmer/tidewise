//! The Startup apps screen: what the rows, the summary and the toast say, from the autostart entries.
//! Plain data; `view.rs` copies it into Slint. No "impact" numbers: Tidewise cannot measure what an
//! app costs at login honestly yet, so none are shown.

use tidewise_adapters::autostart::{Scope, StartupEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The file name, e.g. `discord.desktop`: what the switch acts on.
    pub file: String,
    pub name: String,
    /// `Chat app · Added by you or by the app itself`
    pub desc: String,
    /// The file, shown behind "Show technical details".
    pub path: String,
    /// First letter, upper case.
    pub letter: String,
    pub hue: i32,
    pub enabled: bool,
    /// `Part of your desktop`, or empty.
    pub chip: String,
    /// No switch: refused in code as well.
    pub locked: bool,
    /// `Start Discord at login`
    pub switch_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Screen {
    pub rows: Vec<Row>,
    /// `5` (of the apps below that start at login).
    pub on: String,
    /// `of 8 apps`
    pub of: String,
}

pub fn screen(entries: &[StartupEntry]) -> Screen {
    let rows: Vec<Row> = entries
        .iter()
        .map(|e| {
            let origin = match e.scope {
                Scope::User => "Added by you or by the app itself",
                Scope::System => "Provided by your system",
            };
            // A user copy of a system file is still "provided by your system" to the person.
            let origin = if e.scope == Scope::User && e.has_system_copy {
                "Provided by your system"
            } else {
                origin
            };
            Row {
                file: e.file.clone(),
                name: e.name.clone(),
                desc: match &e.comment {
                    Some(c) => format!("{c} · {origin}"),
                    None => origin.to_string(),
                },
                path: e.path.display().to_string(),
                letter: e
                    .name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().collect())
                    .unwrap_or_default(),
                hue: (e.name.to_lowercase().bytes().map(u32::from).sum::<u32>() % 6) as i32,
                enabled: e.enabled,
                chip: if e.protected {
                    "Part of your desktop".into()
                } else {
                    String::new()
                },
                locked: e.protected,
                switch_label: format!("Start {} at login", e.name),
            }
        })
        .collect();
    let on = rows.iter().filter(|r| r.enabled).count();
    Screen {
        on: on.to_string(),
        of: format!(
            "of {} app{}",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" }
        ),
        rows,
    }
}

/// Where startup files live: the system autostart folders (`$XDG_CONFIG_DIRS`, default `/etc/xdg`,
/// each with `/autostart`), the user's, and where Tidewise keeps backups of user files it edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub system: Vec<std::path::PathBuf>,
    pub user: std::path::PathBuf,
    pub backup: std::path::PathBuf,
}

pub fn paths_from(
    config_dirs: Option<&str>,
    config_home: Option<&str>,
    state_home: Option<&str>,
    home: &std::path::Path,
) -> Paths {
    let absolute = |s: Option<&str>| s.map(std::path::PathBuf::from).filter(|p| p.is_absolute());
    let mut system: Vec<std::path::PathBuf> = config_dirs
        .unwrap_or("")
        .split(':')
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .map(|p| p.join("autostart"))
        .collect();
    if system.is_empty() {
        system.push("/etc/xdg/autostart".into());
    }
    Paths {
        system,
        user: absolute(config_home)
            .unwrap_or_else(|| home.join(".config"))
            .join("autostart"),
        backup: absolute(state_home)
            .unwrap_or_else(|| home.join(".local/state"))
            .join("tidewise/backup"),
    }
}

/// The toast after a switch was flipped.
pub fn toast_text(name: &str, now_on: bool) -> String {
    if now_on {
        format!("{name} will start at login again.")
    } else {
        format!("{name} won't start at login anymore.")
    }
}

/// A plain sentence for a switch that could not be flipped.
pub fn failure_text(name: &str, why: &str) -> String {
    format!("Couldn't change {name}: {why}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(
        file: &str,
        name: &str,
        comment: Option<&str>,
        scope: Scope,
        enabled: bool,
        protected: bool,
    ) -> StartupEntry {
        StartupEntry {
            file: file.into(),
            name: name.into(),
            comment: comment.map(Into::into),
            exec: "x".into(),
            icon: None,
            path: PathBuf::from(match scope {
                Scope::User => format!("/home/u/.config/autostart/{file}"),
                Scope::System => format!("/etc/xdg/autostart/{file}"),
            }),
            scope,
            enabled,
            protected,
            has_system_copy: scope == Scope::System,
        }
    }

    fn sample() -> Vec<StartupEntry> {
        vec![
            entry(
                "discord.desktop",
                "Discord",
                Some("Chat app"),
                Scope::User,
                true,
                false,
            ),
            entry(
                "baloo_file.desktop",
                "Baloo File Daemon",
                None,
                Scope::System,
                false,
                false,
            ),
            entry(
                "org.kde.plasmashell.desktop",
                "Plasma",
                Some("Your desktop"),
                Scope::System,
                true,
                true,
            ),
        ]
    }

    #[test]
    fn the_summary_counts_what_starts_at_login() {
        let s = screen(&sample());
        assert_eq!((s.on.as_str(), s.of.as_str()), ("2", "of 3 apps"));
        let one = screen(&sample()[..1]);
        assert_eq!(one.of, "of 1 app");
        assert_eq!(screen(&[]).of, "of 0 apps");
    }

    #[test]
    fn a_row_has_a_letter_a_stable_colour_a_description_and_a_switch_label() {
        let s = screen(&sample());
        let d = &s.rows[0];
        assert_eq!(
            (d.name.as_str(), d.letter.as_str(), d.file.as_str()),
            ("Discord", "D", "discord.desktop")
        );
        assert_eq!(d.desc, "Chat app · Added by you or by the app itself");
        assert_eq!(d.switch_label, "Start Discord at login");
        assert_eq!(d.path, "/home/u/.config/autostart/discord.desktop");
        assert!(d.enabled && !d.locked && d.chip.is_empty());
        assert!((0..6).contains(&d.hue));
        assert_eq!(d.hue, screen(&sample()).rows[0].hue);
        let b = &s.rows[1];
        assert_eq!(
            b.desc, "Provided by your system",
            "no comment: just where it comes from"
        );
        assert!(!b.enabled);
    }

    #[test]
    fn protected_entries_are_marked_as_part_of_the_desktop_and_have_no_switch() {
        let p = &screen(&sample()).rows[2];
        assert_eq!(p.chip, "Part of your desktop");
        assert!(p.locked);
        assert_eq!(p.desc, "Your desktop · Provided by your system");
    }

    #[test]
    fn startup_folders_follow_the_xdg_variables_with_safe_defaults() {
        let home = std::path::Path::new("/home/u");
        let p = paths_from(None, None, None, home);
        assert_eq!(p.system, [PathBuf::from("/etc/xdg/autostart")]);
        assert_eq!(p.user, PathBuf::from("/home/u/.config/autostart"));
        assert_eq!(
            p.backup,
            PathBuf::from("/home/u/.local/state/tidewise/backup")
        );
        let p = paths_from(Some("/a:/b:relative::"), Some("/cfg"), Some("/state"), home);
        assert_eq!(
            p.system,
            [PathBuf::from("/a/autostart"), PathBuf::from("/b/autostart")]
        );
        assert_eq!(p.user, PathBuf::from("/cfg/autostart"));
        assert_eq!(p.backup, PathBuf::from("/state/tidewise/backup"));
        let p = paths_from(Some(""), Some("relative"), Some(""), home);
        assert_eq!(p.system, [PathBuf::from("/etc/xdg/autostart")]);
        assert_eq!(p.user, PathBuf::from("/home/u/.config/autostart"));
    }

    #[test]
    fn the_toast_says_what_will_happen_at_login() {
        assert_eq!(
            toast_text("Discord", false),
            "Discord won't start at login anymore."
        );
        assert_eq!(
            toast_text("Discord", true),
            "Discord will start at login again."
        );
        assert_eq!(
            failure_text("Discord", "disk full"),
            "Couldn't change Discord: disk full"
        );
    }
}
