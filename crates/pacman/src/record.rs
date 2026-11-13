//! Splits `pacman -Qi` / `pacman -Si` text into records of `Key : value` pairs.

use crate::ParseError;

/// One record of `-Qi` or `-Si` output: its `Key : value` pairs, in the order pacman printed them.
pub(crate) struct Record {
    pub(crate) fields: Vec<(String, String)>,
}

impl Record {
    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Cuts the text into records (same layout for `-Qi` and `-Si`). A blank line ends a record; so does the end of the text.
pub(crate) fn split_records(text: &str) -> Result<Vec<Record>, ParseError> {
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
                    package: name_so_far(&fields),
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
                package: name_so_far(&fields),
                line_number: index + 1,
                line: line.to_string(),
            });
        };
        let key = key.trim();
        if fields.iter().any(|(existing, _)| existing == key) {
            return Err(ParseError::DuplicateField {
                package: name_so_far(&fields),
                field: key.to_string(),
            });
        }
        fields.push((key.to_string(), value.trim().to_string()));
    }
    // The last record may have no blank line after it; that is not an error.
    if !fields.is_empty() {
        records.push(Record { fields });
    }
    Ok(records)
}

/// The `Name` of the record being read, if its `Name` line has been seen yet (for error messages).
fn name_so_far(fields: &[(String, String)]) -> Option<String> {
    fields
        .iter()
        .find(|(key, _)| key == "Name")
        .map(|(_, value)| value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZSH: &str = include_str!("../../../tests/fixtures/pacman/qi-zsh.txt");

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
}
