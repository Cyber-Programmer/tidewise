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

/// Cuts the text into records. A blank line ends a record; so does the end of the text.
fn split_records(text: &str) -> Result<Vec<Record>, ParseError> {
    let mut records = Vec::new();
    let mut fields: Vec<(String, String)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            if !fields.is_empty() {
                records.push(Record {
                    fields: std::mem::take(&mut fields),
                });
            }
            continue;
        }
        // An indented line continues the previous field's value (one value line per line).
        if line.starts_with(char::is_whitespace) {
            let Some((_, value)) = fields.last_mut() else {
                return Err(ParseError::MalformedLine {
                    package: None,
                    line_number: index + 1,
                    line: line.to_string(),
                });
            };
            value.push('\n');
            value.push_str(line.trim());
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
    // The last record may have no blank line after it; that is not an error.
    if !fields.is_empty() {
        records.push(Record { fields });
    }
    Ok(records)
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

    #[test]
    fn qi_description_keeps_colons() {
        let text = include_str!("../../../tests/fixtures/pacman/qi-qt6-imageformats.txt");
        let packages = parse_qi(text).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "qt6-imageformats");
        assert_eq!(
            packages[0].description,
            "Plugins for additional image formats: TIFF, MNG, TGA, WBMP"
        );
        // 269.37 KiB = 275_834.88 bytes
        assert_eq!(packages[0].installed_size, 275_835);
        assert_eq!(packages[0].reason, InstallReason::Dependency);
    }

    #[test]
    fn records_join_continuation_lines() {
        let records = split_records(ZSH).unwrap();
        assert_eq!(records.len(), 1);
        let optional = records[0].get("Optional Deps").unwrap();
        let lines: Vec<&str> = optional.lines().collect();
        assert_eq!(lines.len(), 8);
        assert_eq!(lines[0], "grml-zsh-config: grml's zsh setup");
        assert_eq!(lines[7], "zshdb: A debugger for zsh scripts");
        // The continuation lines must not turn into fields of their own.
        assert_eq!(records[0].get("zsh-doc"), None);
        assert_eq!(records[0].get("Required By"), Some("zsh-completions"));
    }

    #[test]
    fn continuation_before_any_field_is_malformed() {
        let err = split_records("  stray line\nName : zsh\n").err().unwrap();
        assert_eq!(
            err,
            ParseError::MalformedLine {
                package: None,
                line_number: 1,
                line: "  stray line".to_string(),
            }
        );
    }

    const MULTI: &str = include_str!("../../../tests/fixtures/pacman/qi-multi.txt");

    #[test]
    fn qi_multi_three_records_in_order() {
        let packages = parse_qi(MULTI).unwrap();
        let summary: Vec<(&str, u64, InstallReason)> = packages
            .iter()
            .map(|p| (p.name.as_str(), p.installed_size, p.reason))
            .collect();
        assert_eq!(
            summary,
            [
                ("base", 0, InstallReason::Explicit),
                ("tzdata", 1_742_572, InstallReason::Dependency),
                ("linux-api-headers", 7_340_032, InstallReason::Dependency),
            ]
        );
    }

    #[test]
    fn qi_without_trailing_blank_line() {
        let with_blank = parse_qi(ZSH).unwrap();
        let one_newline = ZSH.trim_end_matches('\n').to_string() + "\n";
        let no_newline = ZSH.trim_end_matches('\n');
        assert_eq!(parse_qi(&one_newline).unwrap(), with_blank);
        assert_eq!(parse_qi(no_newline).unwrap(), with_blank);
    }

    #[test]
    fn qi_extra_blank_lines_between_records_are_fine() {
        let spaced = MULTI.replace("\n\n", "\n\n\n\n");
        assert_eq!(parse_qi(&spaced).unwrap(), parse_qi(MULTI).unwrap());
    }

    #[test]
    fn qi_empty_input_is_no_packages() {
        assert_eq!(parse_qi("").unwrap(), []);
        assert_eq!(parse_qi("\n\n\n").unwrap(), []);
    }

    const BROKEN: &str = include_str!("../../../tests/fixtures/pacman/qi-broken-truncated.txt");

    fn missing(package: Option<&str>, field: &'static str) -> ParseError {
        ParseError::MissingField {
            package: package.map(str::to_string),
            field,
        }
    }

    fn bad_value(package: Option<&str>, field: &'static str, value: &str) -> ParseError {
        ParseError::BadValue {
            package: package.map(str::to_string),
            field,
            value: value.to_string(),
        }
    }

    #[test]
    fn qi_broken_truncated_is_missing_field() {
        assert_eq!(
            parse_qi(BROKEN),
            Err(missing(Some("zsh"), "Installed Size"))
        );
    }

    #[test]
    fn qi_one_bad_record_fails_all() {
        let text = MULTI.replace("1701.73 KiB", "lots");
        assert_eq!(
            parse_qi(&text),
            Err(bad_value(Some("tzdata"), "Installed Size", "lots"))
        );
    }

    #[test]
    fn qi_unknown_install_reason_is_bad_value() {
        let text = ZSH.replace("Explicitly installed", "Because I said so");
        assert_eq!(
            parse_qi(&text),
            Err(bad_value(
                Some("zsh"),
                "Install Reason",
                "Because I said so"
            ))
        );
    }

    #[test]
    fn qi_missing_name_has_no_package() {
        let text = ZSH.replace("Name            : zsh\n", "");
        assert_eq!(parse_qi(&text), Err(missing(None, "Name")));
    }

    #[test]
    fn qi_bad_name_is_bad_value() {
        let text = ZSH.replace("Name            : zsh", "Name            : --force");
        assert_eq!(parse_qi(&text), Err(bad_value(None, "Name", "--force")));
    }

    #[test]
    fn qi_missing_or_empty_version_is_an_error() {
        let no_version = ZSH.replace("Version         : 5.9.2-1\n", "");
        assert_eq!(parse_qi(&no_version), Err(missing(Some("zsh"), "Version")));
        let empty = ZSH.replace("5.9.2-1", "");
        assert_eq!(parse_qi(&empty), Err(bad_value(Some("zsh"), "Version", "")));
    }

    #[test]
    fn qi_duplicate_field_is_error() {
        let text = ZSH.replace("Licenses        : custom", "Version         : 9-9");
        assert_eq!(
            parse_qi(&text),
            Err(ParseError::DuplicateField {
                package: Some("zsh".to_string()),
                field: "Version".to_string(),
            })
        );
    }

    #[test]
    fn qi_line_without_colon_is_malformed() {
        let text = "Name            : zsh\nVersion 5.9.2-1\n";
        assert_eq!(
            parse_qi(text),
            Err(ParseError::MalformedLine {
                package: Some("zsh".to_string()),
                line_number: 2,
                line: "Version 5.9.2-1".to_string(),
            })
        );
    }

    #[test]
    fn error_messages_name_package_and_field() {
        assert_eq!(
            missing(Some("zsh"), "Installed Size").to_string(),
            "package \"zsh\": missing field \"Installed Size\""
        );
        assert_eq!(
            bad_value(Some("tzdata"), "Installed Size", "lots").to_string(),
            "package \"tzdata\": field \"Installed Size\" has a value that can't be read: \"lots\""
        );
        assert_eq!(
            missing(None, "Name").to_string(),
            "record without a package name: missing field \"Name\""
        );
        assert_eq!(
            ParseError::BadName {
                line_number: 3,
                name: "--force".to_string()
            }
            .to_string(),
            "line 3: \"--force\" is not a valid package name"
        );
    }
}
