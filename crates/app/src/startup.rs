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

pub fn screen(_entries: &[StartupEntry]) -> Screen {
    Screen::default()
}

/// The toast after a switch was flipped.
pub fn toast_text(_name: &str, _now_on: bool) -> String {
    String::new()
}

/// A plain sentence for a switch that could not be flipped.
pub fn failure_text(_name: &str, _why: &str) -> String {
    String::new()
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
