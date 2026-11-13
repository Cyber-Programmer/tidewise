//! Plain-language words for a pacman failure, shared by the Apps and Search screens.

use tidewise_pacman::PacmanError;

/// Plain-language text for a failure, plus the technical line behind the "details" switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorText {
    pub title: String,
    pub body: String,
    pub technical: String,
}

/// Turns a pacman failure into the words the screen shows, plus the technical line for the details switch.
pub fn error_text(error: &PacmanError) -> ErrorText {
    match error {
        PacmanError::NotFound { .. } => ErrorText {
            title: "Tidewise can't find pacman".to_string(),
            body: "Tidewise is made for Arch Linux and reads your apps through pacman, \
                   but there is no pacman at /usr/bin/pacman on this system."
                .to_string(),
            technical: error.to_string(),
        },
        PacmanError::Spawn { .. } => ErrorText {
            title: "Tidewise couldn't start pacman".to_string(),
            body: "pacman is there but wouldn't start. Try again, or check its permissions."
                .to_string(),
            technical: error.to_string(),
        },
        PacmanError::Failed { code, stderr } => {
            let how = match code {
                Some(code) => format!("exit code {code}"),
                None => "stopped by a signal".to_string(),
            };
            let stderr = stderr.trim();
            ErrorText {
                title: "pacman couldn't list your packages".to_string(),
                body: "pacman stopped with an error before it finished. \
                       This is usually temporary; trying again often works."
                    .to_string(),
                technical: if stderr.is_empty() {
                    how
                } else {
                    format!("{how}: {stderr}")
                },
            }
        }
        PacmanError::Parse(parse_error) => ErrorText {
            title: "Tidewise couldn't read pacman's answer".to_string(),
            body: "The list from pacman looked incomplete, so Tidewise shows nothing \
                   rather than a list with packages missing."
                .to_string(),
            technical: parse_error.to_string(),
        },
        // Not reachable from the Apps list yet (it never searches); the search screen (4.6) will use it.
        PacmanError::BadQuery { .. } => ErrorText {
            title: "That search can't be used".to_string(),
            body: "Tidewise didn't search because the text was too long or had characters \
                   it can't use. Try a shorter, simpler search."
                .to_string(),
            technical: error.to_string(),
        },
        // The details screen (4.6) only passes names that pacman itself printed, so this is a bug in Tidewise.
        PacmanError::BadPackageRef { .. } => ErrorText {
            title: "Tidewise asked pacman about an invalid package".to_string(),
            body: "This is a bug in Tidewise, not something you did. Please report it.".to_string(),
            technical: error.to_string(),
        },
        // The Home screen (M4) will show this when the package lists can't be checked.
        PacmanError::ReadLists { .. } => ErrorText {
            title: "Tidewise couldn't read your package lists".to_string(),
            body: "Tidewise couldn't check how up to date your package lists are, because one of \
                   pacman's database files couldn't be read. Try again, or check the folder's permissions."
                .to_string(),
            technical: error.to_string(),
        },
    }
}
