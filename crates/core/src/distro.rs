//! Which Linux this is, which package tools exist and which desktop is running.

use std::path::Path;

/// The fields of `/etc/os-release` Tidewise cares about.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OsRelease {
    pub name: String,
    pub id: String,
    pub id_like: Vec<String>,
    pub version: Option<String>,
}

pub fn parse_os_release(text: &str) -> OsRelease {
    let mut release = OsRelease::default();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        match key.trim() {
            "NAME" => release.name = value.to_string(),
            "ID" => release.id = value.to_string(),
            "ID_LIKE" => release.id_like = value.split_whitespace().map(String::from).collect(),
            "VERSION_ID" if !value.is_empty() => release.version = Some(value.to_string()),
            _ => {}
        }
    }
    release
}

/// Which package and app tools are installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tools {
    pub pacman: bool,
    pub apt: bool,
    pub dnf: bool,
    pub flatpak: bool,
    pub snap: bool,
}

/// Look for each tool at `<root>/usr/bin/<name>`. Use `/` on a real system.
pub fn detect_tools(root: &Path) -> Tools {
    let has = |name: &str| root.join("usr/bin").join(name).exists();
    Tools {
        pacman: has("pacman"),
        apt: has("apt"),
        dnf: has("dnf"),
        flatpak: has("flatpak"),
        snap: has("snap"),
    }
}

/// Split `XDG_CURRENT_DESKTOP` (for example `ubuntu:GNOME`) into its parts.
pub fn parse_desktop(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(':')
        .filter(|part| !part.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn fixture(name: &str) -> OsRelease {
        let path = format!(
            "{}/../../tests/fixtures/os-release/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        parse_os_release(&std::fs::read_to_string(path).unwrap())
    }

    fn expect(name: &str, id: &str, like: &[&str], version: Option<&str>) -> OsRelease {
        OsRelease {
            name: name.into(),
            id: id.into(),
            id_like: like.iter().map(|s| s.to_string()).collect(),
            version: version.map(String::from),
        }
    }

    #[test]
    fn arch_has_no_version() {
        assert_eq!(fixture("arch"), expect("Arch Linux", "arch", &[], None));
    }

    #[test]
    fn manjaro_is_like_arch() {
        assert_eq!(
            fixture("manjaro"),
            expect("Manjaro Linux", "manjaro", &["arch"], None)
        );
    }

    #[test]
    fn ubuntu_versions() {
        assert_eq!(
            fixture("ubuntu-22.04"),
            expect("Ubuntu", "ubuntu", &["debian"], Some("22.04"))
        );
        assert_eq!(
            fixture("ubuntu-24.04"),
            expect("Ubuntu", "ubuntu", &["debian"], Some("24.04"))
        );
    }

    #[test]
    fn debian_12() {
        assert_eq!(
            fixture("debian-12"),
            expect("Debian GNU/Linux", "debian", &[], Some("12"))
        );
    }

    #[test]
    fn mint_has_two_parents() {
        assert_eq!(
            fixture("mint"),
            expect(
                "Linux Mint",
                "linuxmint",
                &["ubuntu", "debian"],
                Some("21.3")
            )
        );
    }

    #[test]
    fn fedora_unquoted_version() {
        assert_eq!(
            fixture("fedora"),
            expect("Fedora Linux", "fedora", &[], Some("40"))
        );
    }

    #[test]
    fn empty_file_gives_empty_release() {
        assert_eq!(parse_os_release(""), OsRelease::default());
    }

    #[test]
    fn comments_blank_lines_and_single_quotes() {
        let r = parse_os_release("# c\n\nNAME='Odd One'\nID=odd\nnot a pair\n");
        assert_eq!(r, expect("Odd One", "odd", &[], None));
    }

    #[test]
    fn tools_found_under_usr_bin() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("usr/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("pacman"), "").unwrap();
        std::fs::write(bin.join("flatpak"), "").unwrap();
        assert_eq!(
            detect_tools(dir.path()),
            Tools {
                pacman: true,
                flatpak: true,
                ..Tools::default()
            }
        );
    }

    #[test]
    fn no_usr_bin_means_no_tools() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_tools(dir.path()), Tools::default());
    }

    #[test]
    fn desktop_is_split_on_colons() {
        assert_eq!(parse_desktop(Some("ubuntu:GNOME")), vec!["ubuntu", "GNOME"]);
        assert_eq!(parse_desktop(Some("KDE")), vec!["KDE"]);
    }

    #[test]
    fn missing_or_empty_desktop_is_empty() {
        assert!(parse_desktop(None).is_empty());
        assert!(parse_desktop(Some("")).is_empty());
    }
}
