//! Parser for `pacman -Qi`: records of `Key : value` lines, separated by blank lines.

use crate::ParseError;

/// Why a package is on the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallReason {
    /// The user asked for it.
    Explicit,
    /// Pulled in because another package needs it.
    Dependency,
}

/// The facts the Apps list needs about one installed package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    /// Empty if the record has no description.
    pub description: String,
    /// Installed size in bytes (pacman's KiB/MiB/GiB are 1024-based).
    pub installed_size: u64,
    pub reason: InstallReason,
}

/// Parses the output of `LC_ALL=C pacman -Qi ...` into one entry per record.
///
/// All-or-nothing: the first bad record makes the whole call an error naming the package and field.
pub fn parse_qi(_text: &str) -> Result<Vec<InstalledPackage>, ParseError> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZSH: &str = include_str!("../../../tests/fixtures/pacman/qi-zsh.txt");

    #[test]
    fn qi_zsh_fields() {
        let packages = parse_qi(ZSH).unwrap();
        assert_eq!(
            packages,
            [InstalledPackage {
                name: "zsh".to_string(),
                version: "5.9.2-1".to_string(),
                description:
                    "A very advanced and programmable command interpreter (shell) for UNIX"
                        .to_string(),
                installed_size: 7_277_117,
                reason: InstallReason::Explicit,
            }]
        );
    }
}
