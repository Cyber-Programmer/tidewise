//! Plain-language words for a pacman failure, shared by the Apps and Search screens.

use tidewise_pacman::PacmanError;

/// Plain-language text for a failure, plus the technical line behind the "details" switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorText {
    pub title: String,
    pub body: String,
    pub technical: String,
}

/// What Tidewise was asking pacman for when it failed, so the title can say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doing {
    /// The Apps list: `pacman -Qi`.
    ListingInstalled,
    /// A search: `pacman -Ss`.
    Searching,
    /// The details of one package: `pacman -Si`.
    LookingUp,
}

/// Turns a pacman failure into the words the screen shows, plus the technical line for the details switch.
pub fn error_text(error: &PacmanError, doing: Doing) -> ErrorText {
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
                title: match doing {
                    Doing::ListingInstalled => "pacman couldn't list your packages",
                    Doing::Searching => "pacman couldn't finish the search",
                    Doing::LookingUp => "pacman couldn't look up this package",
                }
                .to_string(),
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
            body: match doing {
                Doing::ListingInstalled => {
                    "The list from pacman looked incomplete, so Tidewise shows nothing \
                     rather than a list with packages missing."
                }
                Doing::Searching => {
                    "The search results from pacman looked incomplete, so Tidewise shows none \
                     rather than a list with packages missing."
                }
                Doing::LookingUp => {
                    "The details from pacman looked incomplete, so Tidewise shows none \
                     rather than details with something missing."
                }
            }
            .to_string(),
            technical: parse_error.to_string(),
        },
        // Only the Search screen can get this: the Apps list never searches.
        PacmanError::BadQuery { .. } => ErrorText {
            title: "That search can't be used".to_string(),
            body: "Tidewise didn't search because the text was too long or had characters \
                   it can't use. Try a shorter, simpler search."
                .to_string(),
            technical: error.to_string(),
        },
        // The Search screen only passes names that pacman itself printed, so this is a bug in Tidewise.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn failed() -> PacmanError {
        PacmanError::Failed {
            code: Some(1),
            stderr: "error: unable to lock database\n".to_string(),
        }
    }

    fn parse_failed() -> PacmanError {
        PacmanError::Parse(tidewise_pacman::ParseError::MissingField {
            package: Some("zsh".to_string()),
            field: "Description",
        })
    }

    const FAILED_BODY: &str = "pacman stopped with an error before it finished. \
                               This is usually temporary; trying again often works.";

    #[test]
    fn the_apps_list_keeps_its_words() {
        let text = error_text(&failed(), Doing::ListingInstalled);
        assert_eq!(text.title, "pacman couldn't list your packages");
        assert_eq!(text.body, FAILED_BODY);
        let text = error_text(&parse_failed(), Doing::ListingInstalled);
        assert_eq!(
            text.body,
            "The list from pacman looked incomplete, so Tidewise shows nothing \
             rather than a list with packages missing."
        );
    }

    #[test]
    fn a_failed_search_says_the_search_failed() {
        let text = error_text(&failed(), Doing::Searching);
        assert_eq!(text.title, "pacman couldn't finish the search");
        assert_eq!(text.body, FAILED_BODY);
        assert_eq!(
            text.technical,
            "exit code 1: error: unable to lock database"
        );
    }

    #[test]
    fn a_failed_lookup_says_the_lookup_failed() {
        let text = error_text(&failed(), Doing::LookingUp);
        assert_eq!(text.title, "pacman couldn't look up this package");
        assert_eq!(text.body, FAILED_BODY);
    }

    #[test]
    fn an_unreadable_search_answer_says_results() {
        let text = error_text(&parse_failed(), Doing::Searching);
        assert_eq!(text.title, "Tidewise couldn't read pacman's answer");
        assert_eq!(
            text.body,
            "The search results from pacman looked incomplete, so Tidewise shows none \
             rather than a list with packages missing."
        );
        assert!(text.technical.contains("zsh"));
    }

    #[test]
    fn an_unreadable_lookup_answer_says_details() {
        let text = error_text(&parse_failed(), Doing::LookingUp);
        assert_eq!(text.title, "Tidewise couldn't read pacman's answer");
        assert_eq!(
            text.body,
            "The details from pacman looked incomplete, so Tidewise shows none \
             rather than details with something missing."
        );
    }

    #[test]
    fn failures_that_do_not_depend_on_the_task_read_the_same() {
        let missing = PacmanError::NotFound {
            program: "/usr/bin/pacman".into(),
        };
        let apps = error_text(&missing, Doing::ListingInstalled);
        assert_eq!(error_text(&missing, Doing::Searching), apps);
        assert_eq!(error_text(&missing, Doing::LookingUp), apps);
    }
}
