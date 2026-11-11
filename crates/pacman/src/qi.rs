//! Parser for `pacman -Qi`: records of `Key : value` lines, separated by blank lines.

use crate::ParseError;
use crate::name::is_valid_package_name;
use crate::size::parse_size;

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
pub fn parse_qi(text: &str) -> Result<Vec<InstalledPackage>, ParseError> {
    split_records(text)?.iter().map(build_package).collect()
}

/// One `-Qi` record: its `Key : value` pairs, in the order pacman printed them.
struct Record {
    fields: Vec<(String, String)>,
}

impl Record {
    fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Cuts the text into records. (First version: treats all lines as one record.)
fn split_records(text: &str) -> Result<Vec<Record>, ParseError> {
    let mut fields = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        // Split at the FIRST colon only: values may contain colons too.
        let Some((key, value)) = line.split_once(':') else {
            return Err(ParseError::MalformedLine {
                package: None,
                line_number: index + 1,
                line: line.to_string(),
            });
        };
        fields.push((key.trim().to_string(), value.trim().to_string()));
    }
    if fields.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(vec![Record { fields }])
    }
}

/// Reads the five fields we need out of one record, or says which one is wrong.
fn build_package(record: &Record) -> Result<InstalledPackage, ParseError> {
    let name = record.get("Name").ok_or(ParseError::MissingField {
        package: None,
        field: "Name",
    })?;
    if !is_valid_package_name(name) {
        return Err(ParseError::BadValue {
            package: None,
            field: "Name",
            value: name.to_string(),
        });
    }
    let package = Some(name.to_string());

    let version = record.get("Version").ok_or(ParseError::MissingField {
        package: package.clone(),
        field: "Version",
    })?;
    if version.is_empty() {
        return Err(ParseError::BadValue {
            package,
            field: "Version",
            value: String::new(),
        });
    }

    let size_text = record
        .get("Installed Size")
        .ok_or(ParseError::MissingField {
            package: package.clone(),
            field: "Installed Size",
        })?;
    let installed_size = parse_size(size_text).ok_or_else(|| ParseError::BadValue {
        package: package.clone(),
        field: "Installed Size",
        value: size_text.to_string(),
    })?;

    let reason_text = record
        .get("Install Reason")
        .ok_or(ParseError::MissingField {
            package: package.clone(),
            field: "Install Reason",
        })?;
    let reason = match reason_text {
        "Explicitly installed" => InstallReason::Explicit,
        "Installed as a dependency for another package" => InstallReason::Dependency,
        other => {
            return Err(ParseError::BadValue {
                package,
                field: "Install Reason",
                value: other.to_string(),
            });
        }
    };

    Ok(InstalledPackage {
        name: name.to_string(),
        version: version.to_string(),
        description: record.get("Description").unwrap_or("").to_string(),
        installed_size,
        reason,
    })
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
