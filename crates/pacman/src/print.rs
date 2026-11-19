//! Parser for `pacman -S --print --print-format '%r/%n %v %s'`: the list a dry run would install (WBS 6.1).

use std::collections::HashSet;

use crate::ParseError;
use tidewise_names::{is_valid_package_name, is_valid_repo_name};

/// One line of a dry run: a package pacman would download and install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintedPackage {
    pub repo: String,
    pub name: String,
    /// As pacman prints it (epoch and release included: `1:1.6.9-1`).
    pub version: String,
    /// Bytes still to download (`%s`). 0 when the file is already in the package cache.
    pub download_size: u64,
}

/// Parses the stdout of the dry run into packages, in pacman's order (dependencies first).
///
/// Every line must be `repo/name version size`. Empty text means nothing to install. The first bad line
/// (counted from 1), or a package printed twice, is an error.
pub fn parse_print(text: &str) -> Result<Vec<PrintedPackage>, ParseError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    // One final newline is pacman's; a second one is a blank line, which is an error.
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut packages: Vec<PrintedPackage> = Vec::new();
    let mut names: HashSet<String> = HashSet::new();
    for (index, line) in body.split('\n').enumerate() {
        let package = parse_line(line).ok_or_else(|| ParseError::BadPrintLine {
            line_number: index + 1,
            line: line.to_string(),
        })?;
        // A transaction holds one package of each name.
        if !names.insert(package.name.clone()) {
            return Err(ParseError::UnexpectedPackage {
                repo: package.repo,
                name: package.name,
            });
        }
        packages.push(package);
    }
    Ok(packages)
}

/// One line, or `None` if it is not exactly `repo/name version size`.
///
/// It is read from the right: the size and the version never contain a space, so whatever is left is
/// `repo/name`, even for a repository whose name has a space in it.
fn parse_line(line: &str) -> Option<PrintedPackage> {
    let mut parts = line.rsplitn(3, ' ');
    let size = parts.next()?;
    let version = parts.next()?;
    let spec = parts.next()?;
    let (repo, name) = spec.split_once('/')?;
    // `u64::from_str` also accepts a leading `+`; pacman prints digits only.
    if size.is_empty() || !size.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let download_size = size.parse().ok()?;
    if !is_valid_repo_name(repo)
        || repo.starts_with(' ')
        || repo.ends_with(' ')
        || !is_valid_package_name(name)
        || !is_valid_version(version)
    {
        return None;
    }
    Some(PrintedPackage {
        repo: repo.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        download_size,
    })
}

/// A version as pacman prints it (`epoch:version-release`): ASCII letters and digits and `. _ + ~ : -`, not empty
/// and not starting with `-`. The same rule the helper applies to a confirmed line (`helper-protocol`).
fn is_valid_version(version: &str) -> bool {
    !version.is_empty()
        && !version.starts_with('-')
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '~' | ':' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRON: &str = include_str!("../../../tests/fixtures/pacman/print-cron.txt");
    const PLASMA: &str = include_str!("../../../tests/fixtures/pacman/print-plasma.txt");
    const REINSTALL: &str = include_str!("../../../tests/fixtures/pacman/print-reinstall-zsh.txt");
    const TRUNCATED: &str =
        include_str!("../../../tests/fixtures/pacman/print-broken-truncated.txt");
    const BAD_BYTE: &[u8] = include_bytes!("../../../tests/fixtures/pacman/print-bad-byte.bin");

    fn package(repo: &str, name: &str, version: &str, size: u64) -> PrintedPackage {
        PrintedPackage {
            repo: repo.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            download_size: size,
        }
    }

    fn bad_line(text: &str) -> (usize, String) {
        match parse_print(text) {
            Err(ParseError::BadPrintLine { line_number, line }) => (line_number, line),
            other => panic!("expected BadPrintLine, got {other:?}"),
        }
    }

    #[test]
    fn a_virtual_name_prints_the_provider_and_its_dependency_in_pacmans_order() {
        assert_eq!(
            parse_print(CRON).unwrap(),
            [
                package("extra", "run-parts", "5.24-1", 35779),
                package("extra", "cronie", "1.7.2-2", 92789),
            ]
        );
    }

    #[test]
    fn a_reinstall_looks_like_any_other_line() {
        assert_eq!(
            parse_print(REINSTALL).unwrap(),
            [package("extra", "zsh", "5.9.2-1", 2424190)]
        );
    }

    #[test]
    fn a_group_prints_every_member_and_a_cached_file_has_size_zero() {
        let packages = parse_print(PLASMA).unwrap();
        assert_eq!(packages.len(), 75);
        assert_eq!(packages[0], package("extra", "kdecoration", "6.8.2-1", 0));
        // An epoch stays in the version.
        assert!(packages.iter().any(|p| p.version.contains(':')));
    }

    #[test]
    fn empty_output_is_nothing_to_install() {
        assert_eq!(parse_print("").unwrap(), Vec::new());
    }

    #[test]
    fn the_final_newline_is_optional() {
        let with = parse_print("extra/zsh 5.9.2-1 10\n").unwrap();
        let without = parse_print("extra/zsh 5.9.2-1 10").unwrap();
        assert_eq!(with, without);
        assert_eq!(with.len(), 1);
    }

    #[test]
    fn a_repository_name_with_a_space_is_read_from_the_right() {
        // pacman's own rule allows it; the version and the size never contain a space.
        assert_eq!(
            parse_print("my repo.x/tool 1-1 5\n").unwrap(),
            [package("my repo.x", "tool", "1-1", 5)]
        );
    }

    #[test]
    fn a_blank_line_is_an_error() {
        assert_eq!(bad_line("extra/zsh 5.9-1 1\n\nextra/bc 1.8-1 2\n").0, 2);
        assert_eq!(bad_line("\n").0, 1);
    }

    #[test]
    fn missing_or_extra_words_are_errors() {
        assert_eq!(bad_line("extra/zsh 5.9-1\n").0, 1);
        assert_eq!(bad_line("extra/zsh\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9-1 1 2\n").0, 1);
        // The two-field format of the helper is not this one.
        assert_eq!(bad_line("extra/zsh 5.9-1\n").1, "extra/zsh 5.9-1");
    }

    #[test]
    fn extra_spaces_tabs_and_carriage_returns_are_errors() {
        assert_eq!(bad_line("extra/zsh  5.9-1 1\n").0, 1);
        assert_eq!(bad_line(" extra/zsh 5.9-1 1\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9-1 1 \n").0, 1);
        assert_eq!(bad_line("extra/zsh\t5.9-1 1\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9-1 1\r\n").0, 1);
    }

    #[test]
    fn a_size_must_be_plain_digits_that_fit() {
        assert_eq!(bad_line("extra/zsh 5.9-1 -1\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9-1 1.5\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9-1 +1\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9-1 abc\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9-1 18446744073709551616\n").0, 1);
        assert!(parse_print("extra/zsh 5.9-1 18446744073709551615\n").is_ok());
    }

    #[test]
    fn a_line_without_a_slash_or_with_a_bad_name_or_repository_is_an_error() {
        assert_eq!(bad_line("zsh 5.9-1 1\n").0, 1);
        assert_eq!(bad_line("extra/--force 5.9-1 1\n").0, 1);
        assert_eq!(bad_line("extra/.hidden 5.9-1 1\n").0, 1);
        assert_eq!(bad_line("extra/a/b 5.9-1 1\n").0, 1);
        assert_eq!(bad_line("/zsh 5.9-1 1\n").0, 1);
        assert_eq!(bad_line("local/zsh 5.9-1 1\n").0, 1);
        assert_eq!(bad_line("extra/ 5.9-1 1\n").0, 1);
    }

    #[test]
    fn a_bad_version_is_an_error() {
        assert_eq!(bad_line("extra/zsh -1 1\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9=1 1\n").0, 1);
        assert_eq!(bad_line("extra/zsh 5.9\u{200b}-1 1\n").0, 1);
        assert!(parse_print("extra/zsh 1:5.9~rc1+git_2-1 1\n").is_ok());
    }

    #[test]
    fn the_first_bad_line_is_reported_with_its_number() {
        assert_eq!(
            bad_line("extra/a 1-1 1\nextra/b 1-1 1\nextra/c 1-1\n"),
            (3, "extra/c 1-1".to_string())
        );
    }

    #[test]
    fn the_same_package_twice_is_an_error() {
        let error = parse_print("extra/zsh 5.9-1 1\nextra/zsh 5.9-1 1\n").unwrap_err();
        assert_eq!(
            error,
            ParseError::UnexpectedPackage {
                repo: "extra".to_string(),
                name: "zsh".to_string()
            }
        );
    }

    #[test]
    fn the_truncated_fixture_is_an_error_on_its_cut_line() {
        let (line_number, line) = bad_line(TRUNCATED);
        assert_eq!(line_number, 3);
        assert_eq!(line, "extra/plasma-activitie");
    }

    #[test]
    fn the_bad_byte_fixture_is_an_error_on_the_line_with_the_byte() {
        let text = String::from_utf8_lossy(BAD_BYTE);
        let (line_number, line) = bad_line(&text);
        assert_eq!(line_number, 2);
        assert!(line.contains('\u{fffd}'), "{line:?}");
    }
}
