//! The one error type for every parser in this crate.

use std::fmt;
use std::io;
use std::path::PathBuf;

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
        // `{:?}` on a string shows quotes and escapes control characters, so odd input stays readable.
        match self {
            Self::MissingField { package, field } => {
                write!(f, "{}: missing field {field:?}", who(package))
            }
            Self::BadValue {
                package,
                field,
                value,
            } => write!(
                f,
                "{}: field {field:?} has a value that can't be read: {value:?}",
                who(package)
            ),
            Self::DuplicateField { package, field } => {
                write!(f, "{}: field {field:?} appears twice", who(package))
            }
            Self::MalformedLine {
                package,
                line_number,
                line,
            } => write!(
                f,
                "{}: line {line_number} is not \"Key : value\": {line:?}",
                who(package)
            ),
            Self::BadName { line_number, name } => {
                write!(
                    f,
                    "line {line_number}: {name:?} is not a valid package name"
                )
            }
        }
    }
}

/// How an error refers to the package: by name when we know it.
fn who(package: &Option<String>) -> String {
    match package {
        Some(name) => format!("package {name:?}"),
        None => "record without a package name".to_string(),
    }
}

impl std::error::Error for ParseError {}

/// Why running pacman, or reading its answer, failed.
#[derive(Debug)]
pub enum PacmanError {
    /// The program is not there (spawn failed with "not found").
    NotFound { program: PathBuf },
    /// The program exists but could not be started (for example permission denied).
    Spawn { program: PathBuf, source: io::Error },
    /// pacman ran and exited with an error. `code` is `None` if a signal ended it.
    Failed { code: Option<i32>, stderr: String },
    /// pacman succeeded but its text could not be parsed (the 3.3 error, wrapped).
    Parse(ParseError),
    /// The search text was refused before pacman was started (too long, or it has control characters).
    BadQuery { reason: &'static str },
    /// A repository or package name given to the details lookup was refused before pacman was started.
    /// Callers pass names that came from pacman, so this means a bug in the caller.
    BadPackageRef { reason: &'static str },
}

impl From<ParseError> for PacmanError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl fmt::Display for PacmanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { program } => write!(f, "pacman not found at {}", program.display()),
            Self::Spawn { program, source } => {
                write!(f, "could not start {}: {source}", program.display())
            }
            Self::Failed {
                code: Some(code),
                stderr,
            } => write!(f, "pacman failed (exit code {code}): {stderr}"),
            Self::Failed { code: None, stderr } => {
                write!(f, "pacman was stopped by a signal: {stderr}")
            }
            Self::Parse(error) => write!(f, "could not read pacman's output: {error}"),
            Self::BadQuery { reason } => write!(f, "search text not accepted: {reason}"),
            Self::BadPackageRef { reason } => write!(f, "package reference not accepted: {reason}"),
        }
    }
}

impl std::error::Error for PacmanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Spawn { source, .. } => Some(source),
            Self::Parse(error) => Some(error),
            Self::NotFound { .. }
            | Self::Failed { .. }
            | Self::BadQuery { .. }
            | Self::BadPackageRef { .. } => None,
        }
    }
}
