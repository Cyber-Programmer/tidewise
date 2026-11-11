//! Parser for `pacman -Qq`: one package name per line.

use crate::ParseError;

/// Parses the output of `LC_ALL=C pacman -Qq` into package names, in the order given.
pub fn parse_qq(_text: &str) -> Result<Vec<String>, ParseError> {
    Ok(Vec::new())
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
}
