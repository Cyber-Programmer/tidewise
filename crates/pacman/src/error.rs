//! The one error type for every parser in this crate.

use std::fmt;

/// Why some pacman text could not be parsed. Parsing is all-or-nothing: the first problem is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A required field is absent from a `-Qi` record.
    MissingField {
        package: Option<String>,
        field: &'static str,
    },
    /// A field is there but its value can't be understood.
    BadValue {
        package: Option<String>,
        field: &'static str,
        value: String,
    },
    /// The same field appears twice in one record.
    DuplicateField {
        package: Option<String>,
        field: String,
    },
    /// A line that is neither `Key : value` nor an indented continuation line.
    MalformedLine {
        package: Option<String>,
        line_number: usize,
        line: String,
    },
    /// A `-Qq` line that is not a valid package name.
    BadName { line_number: usize, name: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error")
    }
}

impl std::error::Error for ParseError {}
