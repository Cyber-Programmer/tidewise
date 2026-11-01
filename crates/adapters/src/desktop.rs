//! `.desktop` files: what an app is called, how it starts, which package owns the file. Used to turn
//! the thousands of installed packages into the apps a person would recognise.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::source::AppEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    /// `NoDisplay=true` or `Hidden=true`: it is not shown in menus, so it is not an app.
    pub hidden: bool,
}

/// The `[Desktop Entry]` group of a `.desktop` file. `None` when there is none, when it is not an
/// application, or when it has no plain `Name=`. Translated `Name[xx]=` lines are ignored.
pub fn parse_desktop_file(text: &str) -> Option<DesktopEntry> {
    let _ = text;
    None
}

/// The program an `Exec=` line starts: the first word, unquoted, skipping a leading `env VAR=value`.
pub fn exec_program(exec: &str) -> Option<String> {
    let _ = exec;
    None
}

/// `/path/file.desktop is owned by package 1.2-3` lines of `pacman -Qo`. Error lines are skipped.
pub fn parse_pacman_qo(text: &str) -> Vec<(PathBuf, String)> {
    let _ = text;
    Vec::new()
}

/// `package[, package]: /path` lines of `dpkg -S`. Lines that are not that are skipped.
pub fn parse_dpkg_s(text: &str) -> Vec<(PathBuf, String)> {
    let _ = text;
    Vec::new()
}

/// Keep the packages that own a visible `.desktop` file and name them as the file does; everything
/// else (libraries, tools without a menu entry) is not an app.
pub fn apps_from_packages(
    packages: Vec<AppEntry>,
    owned: &[(PathBuf, String)],
    entries: &HashMap<PathBuf, DesktopEntry>,
) -> Vec<AppEntry> {
    let _ = (owned, entries);
    packages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(rel: &str) -> String {
        let path = format!("{}/../../tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn real_desktop_files_give_name_exec_and_icon() {
        let ff = parse_desktop_file(&fx("desktop/firefox.desktop")).unwrap();
        assert_eq!(ff.name, "Firefox");
        assert_eq!(ff.exec, "/usr/lib/firefox/firefox %u");
        assert_eq!(ff.icon.as_deref(), Some("firefox"));
        assert!(!ff.hidden);
        let code = parse_desktop_file(&fx("desktop/code.desktop")).unwrap();
        assert_eq!(
            (code.name.as_str(), code.icon.as_deref()),
            ("Visual Studio Code", Some("vscode"))
        );
    }

    #[test]
    fn a_broken_file_and_other_types_are_not_apps() {
        assert_eq!(parse_desktop_file(&fx("desktop/broken.desktop")), None);
        assert_eq!(parse_desktop_file(""), None);
        assert_eq!(
            parse_desktop_file("[Desktop Entry]\nType=Link\nName=x\nURL=y\n"),
            None
        );
        assert_eq!(
            parse_desktop_file("[Desktop Entry]\nType=Application\nExec=x\n"),
            None,
            "no Name"
        );
        assert_eq!(
            parse_desktop_file("[Desktop Entry]\nType=Application\nName[de]=Nur\nExec=x\n"),
            None
        );
    }

    #[test]
    fn only_the_desktop_entry_group_counts_and_hidden_is_flagged() {
        let text = "[Desktop Action new]\nName=New window\nExec=x --new\n\n[Desktop Entry]\nType=Application\nName=Tool\nExec=tool\nNoDisplay=true\n";
        let e = parse_desktop_file(text).unwrap();
        assert_eq!(
            (e.name.as_str(), e.exec.as_str(), e.hidden),
            ("Tool", "tool", true)
        );
        let hidden = "[Desktop Entry]\nType=Application\nName=T\nExec=t\nHidden=true\n";
        assert!(parse_desktop_file(hidden).unwrap().hidden);
        let shown = "[Desktop Entry]\nType=Application\nName=T\nExec=t\nNoDisplay=false\n";
        assert!(!parse_desktop_file(shown).unwrap().hidden);
    }

    #[test]
    fn exec_lines_give_the_program() {
        assert_eq!(exec_program("code %F").as_deref(), Some("code"));
        assert_eq!(
            exec_program("\"/home/u/Applications/Krita 5.2.AppImage\" %U").as_deref(),
            Some("/home/u/Applications/Krita 5.2.AppImage")
        );
        assert_eq!(
            exec_program("env GDK_BACKEND=x11 /opt/app/app --flag").as_deref(),
            Some("/opt/app/app")
        );
        assert_eq!(exec_program("  /usr/bin/x").as_deref(), Some("/usr/bin/x"));
        assert_eq!(exec_program(""), None);
        assert_eq!(exec_program("\"unterminated"), None);
        assert_eq!(exec_program("env A=b"), None);
    }

    #[test]
    fn real_pacman_qo_output_is_read_and_errors_are_skipped() {
        let owned = parse_pacman_qo(&fx("desktop-owners/pacman-Qo-real.txt"));
        assert_eq!(owned.len(), 6);
        assert_eq!(
            owned[0],
            (
                PathBuf::from("/usr/share/applications/antigravity-ide-url-handler.desktop"),
                "antigravity-ide".to_string()
            )
        );
        assert_eq!(owned[4].1, "avahi");
        assert_eq!(parse_pacman_qo("error: No package owns /x\n"), vec![]);
        assert_eq!(parse_pacman_qo(""), vec![]);
    }

    #[test]
    fn dpkg_s_lines_with_several_packages_and_arch_suffixes() {
        let owned = parse_dpkg_s(&fx("desktop-owners/dpkg-S-handwritten.txt"));
        let got: Vec<_> = owned
            .iter()
            .map(|(p, n)| (p.to_str().unwrap(), n.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("/usr/share/applications/firefox.desktop", "firefox"),
                ("/usr/share/doc/libc6", "libc6:amd64"),
                ("/usr/share/applications/vlc.desktop", "vlc"),
                ("/usr/share/applications/vlc.desktop", "vlc-bin"),
            ]
        );
        assert_eq!(
            parse_dpkg_s(&fx("desktop-owners/dpkg-S-de-handwritten.txt")).len(),
            1
        );
    }

    fn pkg(name: &str) -> AppEntry {
        AppEntry {
            id: name.into(),
            name: name.into(),
            kind: crate::source::SourceKind::Pacman,
            scope: crate::source::Scope::System,
            version: None,
            size_bytes: Some(1),
            required_by: vec![],
        }
    }

    fn entry(name: &str, hidden: bool) -> DesktopEntry {
        DesktopEntry {
            name: name.into(),
            exec: "x".into(),
            icon: None,
            hidden,
        }
    }

    #[test]
    fn packages_with_a_visible_desktop_file_become_apps_named_like_the_file() {
        let owned = vec![
            (
                "/usr/share/applications/firefox.desktop".into(),
                "firefox".to_string(),
            ),
            (
                "/usr/share/applications/helper.desktop".into(),
                "libfoo".to_string(),
            ),
            (
                "/usr/share/applications/a.desktop".into(),
                "multi".to_string(),
            ),
            (
                "/usr/share/applications/b.desktop".into(),
                "multi".to_string(),
            ),
        ];
        let entries: HashMap<PathBuf, DesktopEntry> = [
            (
                "/usr/share/applications/firefox.desktop",
                entry("Firefox", false),
            ),
            (
                "/usr/share/applications/helper.desktop",
                entry("Helper", true),
            ),
            ("/usr/share/applications/a.desktop", entry("Multi A", false)),
            ("/usr/share/applications/b.desktop", entry("Multi B", false)),
        ]
        .into_iter()
        .map(|(p, e)| (PathBuf::from(p), e))
        .collect();
        let apps = apps_from_packages(
            vec![pkg("bash"), pkg("firefox"), pkg("libfoo"), pkg("multi")],
            &owned,
            &entries,
        );
        let got: Vec<_> = apps
            .iter()
            .map(|a| (a.id.as_str(), a.name.as_str()))
            .collect();
        assert_eq!(got, [("firefox", "Firefox"), ("multi", "Multi A")]);
    }

    #[test]
    fn a_desktop_file_nobody_listed_or_owned_by_an_unknown_package_changes_nothing() {
        let owned = vec![("/x.desktop".into(), "ghost".to_string())];
        let entries: HashMap<PathBuf, DesktopEntry> =
            [(PathBuf::from("/x.desktop"), entry("Ghost", false))].into();
        assert!(apps_from_packages(vec![pkg("bash")], &owned, &entries).is_empty());
        assert!(apps_from_packages(vec![pkg("bash")], &[], &HashMap::new()).is_empty());
    }
}
