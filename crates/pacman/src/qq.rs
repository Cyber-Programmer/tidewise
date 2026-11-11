//! Parser for `pacman -Qq`: one package name per line.

use crate::ParseError;
use crate::name::is_valid_package_name;

/// Parses the output of `LC_ALL=C pacman -Qq` into package names, in the order given.
///
/// Every line must be a valid package name; the first bad line (counted from 1) is an error.
pub fn parse_qq(text: &str) -> Result<Vec<String>, ParseError> {
    let mut names = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if !is_valid_package_name(line) {
            return Err(ParseError::BadName {
                line_number: index + 1,
                name: line.to_string(),
            });
        }
        names.push(line.to_string());
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../tests/fixtures/pacman/qq-sample.txt");

    #[test]
    fn qq_sample_gives_seven_names_in_order() {
        let names = parse_qq(SAMPLE).unwrap();
        assert_eq!(
            names,
            [
                "baobab",
                "base",
                "bc",
                "linux-api-headers",
                "qt6-imageformats",
                "tzdata",
                "zsh"
            ]
        );
    }

    fn bad_name(text: &str) -> (usize, String) {
        match parse_qq(text) {
            Err(ParseError::BadName { line_number, name }) => (line_number, name),
            other => panic!("expected BadName, got {other:?}"),
        }
    }

    #[test]
    fn qq_rejects_empty_line() {
        assert_eq!(bad_name("zsh\n\nbc\n"), (2, String::new()));
    }

    #[test]
    fn qq_rejects_name_with_whitespace() {
        assert_eq!(bad_name("zsh\nfoo bar\n"), (2, "foo bar".to_string()));
        assert_eq!(bad_name(" zsh\n"), (1, " zsh".to_string()));
    }

    #[test]
    fn qq_rejects_name_starting_with_dash() {
        assert_eq!(bad_name("bc\nzsh\n--force\n"), (3, "--force".to_string()));
    }

    #[test]
    fn qq_empty_input_is_empty_list() {
        assert_eq!(parse_qq("").unwrap(), Vec::<String>::new());
    }
}
