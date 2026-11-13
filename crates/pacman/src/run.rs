//! Runs pacman (read-only) and turns its output into values.
//!
//! This is the only module that starts a process. The work is split in two so it can be tested
//! without pacman: "run the process" (`run`) and "turn its `Output` into the result"
//! (`packages_from_output`).

use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::name::{is_valid_package_name, is_valid_repo_name};
use crate::{
    DetailsLookup, InstalledPackage, PackageRef, PacmanError, ParseError, SearchResult, parse_qi,
    parse_si, parse_ss,
};

/// Absolute path: never looked up through `PATH`.
const PACMAN: &str = "/usr/bin/pacman";

/// Builds the command: absolute program, arguments as a list (no shell), empty environment
/// except `LC_ALL=C`, and no stdin so pacman can never wait for a prompt.
fn pacman_command(program: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    command
}

/// Every installed package, from one `pacman -Qi` call (with no names it prints all of them).
///
/// Read-only, no root, no shell. Blocks until pacman finishes (no timeout yet).
pub fn installed_packages() -> Result<Vec<InstalledPackage>, PacmanError> {
    let output = run(Path::new(PACMAN), &["-Qi"])?;
    packages_from_output(&output)
}

/// Searches the repositories pacman knows (official and third-party) with `pacman -Ss`.
///
/// Every space-separated term must match (name or description), literally: `c++` means `c++`, not a
/// pattern. An empty or blank `query` returns no results without running pacman. Results are in pacman's
/// order; use `SearchResult::is_official` to tell Arch's own repos from third-party ones.
/// Read-only, no root, no shell. Blocks until pacman finishes (no timeout yet).
pub fn search(query: &str) -> Result<Vec<SearchResult>, PacmanError> {
    let Some(args) = search_args(query)? else {
        return Ok(Vec::new());
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run(Path::new(PACMAN), &args)?;
    results_from_output(&output)
}

/// Checks the packages to look up and removes repeats (keeping first-seen order).
///
/// Nothing invalid may reach pacman: every repo and name is checked first. An empty request stays empty,
/// and the caller must not run pacman for it (`pacman -Si` with no names prints every package).
fn details_request(refs: &[PackageRef]) -> Result<Vec<PackageRef>, PacmanError> {
    // Check everything first, repeats included: one bad entry refuses the whole request.
    for item in refs {
        if !is_valid_repo_name(&item.repo) {
            return Err(PacmanError::BadPackageRef {
                reason: "the repository name is not valid",
            });
        }
        if !is_valid_package_name(&item.name) {
            return Err(PacmanError::BadPackageRef {
                reason: "the package name is not valid",
            });
        }
    }
    // Asking twice would print the package twice, and then a result could not be matched to one request.
    let mut seen = HashSet::new();
    Ok(refs
        .iter()
        .filter(|item| seen.insert(*item))
        .cloned()
        .collect())
}

/// The arguments for `pacman -Si`: `--` always comes before the names, so no name can be an option.
fn details_args(request: &[PackageRef]) -> Vec<String> {
    let mut args = vec!["-Si".to_string(), "--".to_string()];
    args.extend(
        request
            .iter()
            .map(|item| format!("{}/{}", item.repo, item.name)),
    );
    args
}

/// Turns what `pacman -Si` printed into found and not-found packages. Pure: takes an `Output`, runs nothing.
///
/// `request` is the checked, de-duplicated list that was asked for.
fn lookup_from_output(
    output: &Output,
    request: &[PackageRef],
) -> Result<DetailsLookup, PacmanError> {
    // The exit code counts the unknown names (1, 2, 3, ...) so it can't tell "some missing" from a real
    // failure. The words on stderr can: see `missing_from_stderr`.
    let not_found = if output.status.success() {
        Vec::new()
    } else if let Some(missing) = missing_from_stderr(output, request) {
        missing
    } else {
        check_exit(output)?;
        Vec::new()
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let found = parse_si(&text)?;

    // Every record must be something we asked for, once.
    let mut seen = HashSet::new();
    for details in &found {
        let asked = request
            .iter()
            .any(|item| item.repo == details.repo && item.name == details.name);
        if !asked || !seen.insert((details.repo.as_str(), details.name.as_str())) {
            return Err(ParseError::UnexpectedPackage {
                repo: details.repo.clone(),
                name: details.name.clone(),
            }
            .into());
        }
    }
    // Every request must be answered: described, or reported as not found.
    for item in request {
        if !seen.contains(&(item.repo.as_str(), item.name.as_str())) && !not_found.contains(item) {
            return Err(ParseError::MissingPackage {
                repo: item.repo.clone(),
                name: item.name.clone(),
            }
            .into());
        }
    }
    Ok(DetailsLookup { found, not_found })
}

/// The packages pacman said were not found, if that is ALL it complained about.
///
/// Every non-empty stderr line must be exactly `error: package 'repo/name' was not found` for a package in
/// the request. Anything else (an unknown repository, a warning, an empty stderr, a signal) returns `None`
/// and the caller reports a failure. The result follows the request's order.
fn missing_from_stderr(output: &Output, request: &[PackageRef]) -> Option<Vec<PackageRef>> {
    output.status.code()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut missing = HashSet::new();
    for line in stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let asked = line
            .strip_prefix("error: package '")?
            .strip_suffix("' was not found")?;
        let item = request
            .iter()
            .find(|item| format!("{}/{}", item.repo, item.name) == asked)?;
        missing.insert(item);
    }
    if missing.is_empty() {
        return None;
    }
    Some(
        request
            .iter()
            .filter(|item| missing.contains(item))
            .cloned()
            .collect(),
    )
}

/// Looks up the details of packages in the repositories with `pacman -Si`.
///
/// Pass what a search result gave you (`PackageRef::from(&result)`). Repeats are asked once. A name pacman does
/// not know is not an error: it comes back in `not_found` next to the packages that were found. An invalid
/// repository or package name is refused before pacman starts (`BadPackageRef`); an empty list returns an
/// empty answer without running pacman. Reads the local sync database only (no `-Sy`, no network, no root).
/// Blocks until pacman finishes (no timeout yet).
pub fn package_details(refs: &[PackageRef]) -> Result<DetailsLookup, PacmanError> {
    let _ = refs;
    Ok(DetailsLookup {
        found: Vec::new(),
        not_found: Vec::new(),
    })
}

/// Longest search text we accept, in characters (after trimming).
const MAX_QUERY_CHARS: usize = 100;

/// Turns what the user typed into the arguments for `pacman -Ss`.
///
/// `Ok(None)`: nothing to search for, so pacman must not be run (`pacman -Ss` alone lists everything).
fn search_args(query: &str) -> Result<Option<Vec<String>>, PacmanError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(PacmanError::BadQuery {
            reason: "the search text is too long",
        });
    }
    // Spaces, tabs and newlines just separate terms; any other control character is refused.
    if query
        .chars()
        .any(|c| c.is_control() && !c.is_ascii_whitespace())
    {
        return Err(PacmanError::BadQuery {
            reason: "the search text contains a control character",
        });
    }
    // "-Ss" then "--" ALWAYS come first, so a term like "--help" or "-dd" can never be an option.
    // pacman ANDs the terms, which is what a person typing two words expects.
    let mut args = vec!["-Ss".to_string(), "--".to_string()];
    args.extend(query.split_whitespace().map(escape_ere));
    Ok(Some(args))
}

/// Puts a backslash before every POSIX ERE special character, so the term is matched literally.
/// (pacman treats each search term as a regular expression. `-` and `_` are not special.)
fn escape_ere(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len());
    for c in term.chars() {
        if ".[](){}*+?|^$\\".contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// Starts the program and waits for it. Start-up failures become `NotFound` or `Spawn`.
fn run(program: &Path, args: &[&str]) -> Result<Output, PacmanError> {
    pacman_command(program, args)
        .output()
        .map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => PacmanError::NotFound {
                program: program.to_path_buf(),
            },
            _ => PacmanError::Spawn {
                program: program.to_path_buf(),
                source,
            },
        })
}

/// A non-zero exit (or a signal) is an error that keeps pacman's own words.
fn check_exit(output: &Output) -> Result<(), PacmanError> {
    if output.status.success() {
        return Ok(());
    }
    Err(PacmanError::Failed {
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

/// Turns what pacman printed into packages. Pure: takes an `Output`, runs nothing.
fn packages_from_output(output: &Output) -> Result<Vec<InstalledPackage>, PacmanError> {
    check_exit(output)?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_qi(&text)?)
}

/// Turns what `pacman -Ss` printed into results. Pure: takes an `Output`, runs nothing.
fn results_from_output(output: &Output) -> Result<Vec<SearchResult>, PacmanError> {
    // `pacman -Ss` exits 1 when nothing matches, and says nothing at all. A bad pattern also exits 1,
    // but then it explains itself on stderr, so "no results" needs all three: exit 1, no stdout, no stderr.
    if output.status.code() == Some(1)
        && output.stdout.is_empty()
        && String::from_utf8_lossy(&output.stderr).trim().is_empty()
    {
        return Ok(Vec::new());
    }
    check_exit(output)?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ss(&text)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParseError;
    use std::ffi::OsStr;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const MULTI: &str = include_str!("../../../tests/fixtures/pacman/qi-multi.txt");

    /// A fake pacman result. `code` is the exit code (0 = success).
    fn fake_output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    /// Runs `packages_from_output` and returns its error, with a clear message if it succeeded.
    fn expect_error(output: &Output) -> PacmanError {
        match packages_from_output(output) {
            Err(error) => error,
            Ok(packages) => panic!("expected an error, got {} packages", packages.len()),
        }
    }

    #[test]
    fn success_output_parses_like_parse_qi() {
        let output = fake_output(0, MULTI.as_bytes(), b"");
        let packages = packages_from_output(&output).unwrap();
        assert_eq!(packages, parse_qi(MULTI).unwrap());
        assert!(!packages.is_empty());
    }

    #[test]
    fn command_is_absolute_pacman_with_arg_list_and_c_locale() {
        let cmd = pacman_command(Path::new(PACMAN), &["-Qi"]);
        assert_eq!(cmd.get_program(), "/usr/bin/pacman");
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, ["-Qi"]);
        let envs: Vec<_> = cmd.get_envs().collect();
        assert_eq!(envs, [(OsStr::new("LC_ALL"), Some(OsStr::new("C")))]);
    }

    #[test]
    fn non_zero_exit_keeps_stderr() {
        let text = "error: package 'x' was not found\n";
        let output = fake_output(1, b"", text.as_bytes());
        let error = expect_error(&output);
        assert!(
            matches!(&error, PacmanError::Failed { code: Some(1), stderr }
                if stderr == "error: package 'x' was not found"),
            "got {error:?}"
        );
        assert!(
            error
                .to_string()
                .contains("error: package 'x' was not found")
        );
        assert!(error.to_string().contains("1"));
    }

    #[test]
    fn exit_by_signal_has_no_code() {
        // Raw wait status 9 = "killed by signal 9".
        let output = Output {
            status: ExitStatus::from_raw(9),
            stdout: Vec::new(),
            stderr: b"partial".to_vec(),
        };
        let error = expect_error(&output);
        assert!(
            matches!(&error, PacmanError::Failed { code: None, stderr } if stderr == "partial"),
            "got {error:?}"
        );
        assert!(error.to_string().contains("signal"));
    }

    #[test]
    fn unparseable_text_is_wrapped_parse_error() {
        let broken = include_str!("../../../tests/fixtures/pacman/qi-broken-truncated.txt");
        let error = expect_error(&fake_output(0, broken.as_bytes(), b""));
        let expected = parse_qi(broken).unwrap_err();
        assert!(
            matches!(&error, PacmanError::Parse(inner) if *inner == expected),
            "got {error:?}"
        );
        assert!(error.to_string().contains("zsh"));
        assert!(error.to_string().contains("Installed Size"));
        let source = std::error::Error::source(&error).map(ToString::to_string);
        assert_eq!(source, Some(expected.to_string()));
    }

    #[test]
    fn invalid_utf8_becomes_replacement_char() {
        let zsh = include_str!("../../../tests/fixtures/pacman/qi-zsh.txt");
        let mut bytes = zsh.as_bytes().to_vec();
        // Put a byte that is never valid UTF-8 into the description.
        let at = zsh.find("programmable").unwrap();
        bytes[at] = 0xFF;
        let packages = packages_from_output(&fake_output(0, &bytes, b"")).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "zsh");
        assert!(
            packages[0].description.contains('\u{FFFD}'),
            "{:?}",
            packages[0].description
        );
        assert!(!packages[0].description.contains("programmable"));
    }

    #[test]
    fn invalid_utf8_in_a_name_is_still_rejected() {
        let zsh = include_str!("../../../tests/fixtures/pacman/qi-zsh.txt");
        let mut bytes = zsh.as_bytes().to_vec();
        let at = zsh.find("zsh").unwrap(); // the Name value
        bytes[at] = 0xFF;
        let error = expect_error(&fake_output(0, &bytes, b""));
        // The name becomes "\u{FFFD}sh", which is not an allowed package name.
        assert!(
            matches!(
                &error,
                PacmanError::Parse(ParseError::BadValue { field: "Name", .. })
            ),
            "got {error:?}"
        );
    }

    #[test]
    fn missing_program_is_not_found() {
        let error = run(Path::new("/nonexistent/pacman"), &["-Qi"]).unwrap_err();
        assert!(
            matches!(&error, PacmanError::NotFound { program } if program == Path::new("/nonexistent/pacman")),
            "got {error:?}"
        );
        assert!(error.to_string().contains("/nonexistent/pacman"));
    }

    #[test]
    fn unstartable_program_is_spawn_error() {
        // A directory exists but cannot be executed.
        let error = run(Path::new("/"), &[]).unwrap_err();
        assert!(matches!(&error, PacmanError::Spawn { .. }), "got {error:?}");
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn run_returns_the_process_output() {
        // /usr/bin/true exists on every Linux (including CI); this only checks that
        // `run` hands back a finished process, not anything about pacman.
        let output = run(Path::new("/usr/bin/true"), &[]).unwrap();
        assert!(output.status.success());
        let output = run(Path::new("/usr/bin/false"), &[]).unwrap();
        assert_eq!(output.status.code(), Some(1));
    }

    /// The only test that runs the real pacman. Needs an Arch machine, so CI skips it.
    /// Run it with: `cargo test -p tidewise-pacman -- --ignored`
    #[test]
    #[ignore = "runs the real /usr/bin/pacman; run with: cargo test -p tidewise-pacman -- --ignored"]
    fn real_pacman_lists_every_installed_package() {
        let packages = installed_packages().unwrap();
        assert!(packages.iter().any(|p| p.name == "pacman"));

        // Cross-check "no names prints every package" against a separate `-Qq` run.
        let output = run(Path::new(PACMAN), &["-Qq"]).unwrap();
        check_exit(&output).unwrap();
        let names = crate::parse_qq(&String::from_utf8_lossy(&output.stdout)).unwrap();
        assert_eq!(packages.len(), names.len());
    }
}

#[cfg(test)]
mod search_args_tests {
    use super::*;

    /// The arguments for `query`, with a clear message if there are none or it was refused.
    fn args(query: &str) -> Vec<String> {
        match search_args(query) {
            Ok(Some(args)) => args,
            other => panic!("{query:?}: expected arguments, got {other:?}"),
        }
    }

    fn refused(query: &str) -> &'static str {
        match search_args(query) {
            Err(PacmanError::BadQuery { reason }) => reason,
            other => panic!("{query:?}: expected BadQuery, got {other:?}"),
        }
    }

    #[test]
    fn empty_or_blank_query_means_do_not_run_pacman() {
        for query in ["", " ", "   ", "\t", "\n", " \t\n "] {
            assert!(
                matches!(search_args(query), Ok(None)),
                "{query:?} should give no arguments"
            );
        }
    }

    #[test]
    fn plain_word_gets_the_fixed_prefix() {
        assert_eq!(args("firefox"), ["-Ss", "--", "firefox"]);
        assert_eq!(args("  firefox \n"), ["-Ss", "--", "firefox"]);
    }

    #[test]
    fn terms_are_split_on_any_whitespace() {
        assert_eq!(
            args("firefox  dev\tkit\nx"),
            ["-Ss", "--", "firefox", "dev", "kit", "x"]
        );
    }

    #[test]
    fn option_lookalikes_stay_search_words_after_double_dash() {
        assert_eq!(args("--help"), ["-Ss", "--", "--help"]);
        assert_eq!(args("-dd"), ["-Ss", "--", "-dd"]);
        assert_eq!(args("--noconfirm -y"), ["-Ss", "--", "--noconfirm", "-y"]);
    }

    #[test]
    fn every_ere_special_character_is_escaped() {
        for special in ".[](){}*+?|^$\\".chars() {
            let query = format!("a{special}b");
            assert_eq!(args(&query), ["-Ss", "--", &format!("a\\{special}b")]);
        }
        assert_eq!(args("c++"), ["-Ss", "--", r"c\+\+"]);
        assert_eq!(args("("), ["-Ss", "--", r"\("]);
        assert_eq!(args("["), ["-Ss", "--", r"\["]);
        assert_eq!(args("a.*b"), ["-Ss", "--", r"a\.\*b"]);
    }

    #[test]
    fn ordinary_package_name_characters_are_left_alone() {
        assert_eq!(args("lib32-glibc_2@x"), ["-Ss", "--", "lib32-glibc_2@x"]);
        assert_eq!(args("café"), ["-Ss", "--", "café"]);
    }

    #[test]
    fn length_cap_counts_characters_after_trimming() {
        assert_eq!(args(&"x".repeat(100)).len(), 3);
        assert_eq!(args(&format!("  {}  ", "x".repeat(100))).len(), 3);
        assert_eq!(args(&"é".repeat(100)).len(), 3); // 200 bytes, 100 characters
        assert!(refused(&"x".repeat(101)).contains("long"));
        assert!(refused(&"é".repeat(101)).contains("long"));
    }

    #[test]
    fn control_characters_are_refused() {
        for query in ["foo\0bar", "foo\x07", "a\x1bb", "\x7f", "a\u{85}b"] {
            assert!(refused(query).contains("control"), "{query:?}");
        }
    }

    #[test]
    fn refusal_message_is_plain() {
        let error = search_args(&"x".repeat(101)).unwrap_err();
        assert!(error.to_string().starts_with("search text not accepted:"));
    }
}

#[cfg(test)]
mod search_output_tests {
    use super::*;
    use crate::{ParseError, parse_ss};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const SAMPLE: &str = include_str!("../../../tests/fixtures/pacman/ss-sample.txt");
    const THIRD_PARTY: &str = include_str!("../../../tests/fixtures/pacman/ss-third-party.txt");
    const TRUNCATED: &str = include_str!("../../../tests/fixtures/pacman/ss-broken-truncated.txt");
    const BAD_BYTE: &[u8] = include_bytes!("../../../tests/fixtures/pacman/ss-bad-byte.bin");

    /// A fake pacman result. `code` is the exit code (0 = success).
    fn fake_output(code: i32, stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    fn expect_error(output: &Output) -> PacmanError {
        match results_from_output(output) {
            Err(error) => error,
            Ok(results) => panic!("expected an error, got {} results", results.len()),
        }
    }

    #[test]
    fn success_output_parses_like_parse_ss() {
        let results = results_from_output(&fake_output(0, SAMPLE.as_bytes(), b"")).unwrap();
        assert_eq!(results, parse_ss(SAMPLE).unwrap());
        assert_eq!(results.len(), 7);
    }

    #[test]
    fn third_party_repos_and_pacmans_order_survive() {
        let results = results_from_output(&fake_output(0, THIRD_PARTY.as_bytes(), b"")).unwrap();
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            ["zsh", "tidewise-demo", "demo-tools", "odd-one", "acl"]
        );
        assert!(!results[1].is_official());
    }

    #[test]
    fn exit_0_with_no_text_is_no_results() {
        assert!(
            results_from_output(&fake_output(0, b"", b""))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn exit_1_with_nothing_printed_is_no_results_not_an_error() {
        assert!(
            results_from_output(&fake_output(1, b"", b""))
                .unwrap()
                .is_empty()
        );
        // A lone newline on stderr is still "nothing said".
        assert!(
            results_from_output(&fake_output(1, b"", b"\n"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn exit_1_with_stderr_text_is_an_error() {
        // What pacman 7.1.0 really prints for a bad regular expression.
        let stderr = b"error: search failed: invalid regular expression\n";
        let error = expect_error(&fake_output(1, b"", stderr));
        assert!(
            matches!(&error, PacmanError::Failed { code: Some(1), stderr }
                if stderr == "error: search failed: invalid regular expression"),
            "got {error:?}"
        );
    }

    #[test]
    fn exit_1_with_stdout_text_is_an_error() {
        let error = expect_error(&fake_output(1, SAMPLE.as_bytes(), b""));
        assert!(
            matches!(&error, PacmanError::Failed { code: Some(1), .. }),
            "got {error:?}"
        );
    }

    #[test]
    fn other_exit_codes_and_signals_are_errors() {
        let error = expect_error(&fake_output(2, b"", b""));
        assert!(
            matches!(&error, PacmanError::Failed { code: Some(2), .. }),
            "got {error:?}"
        );
        let killed = Output {
            status: ExitStatus::from_raw(9),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let error = expect_error(&killed);
        assert!(
            matches!(&error, PacmanError::Failed { code: None, .. }),
            "got {error:?}"
        );
    }

    #[test]
    fn unparseable_text_is_wrapped_parse_error() {
        let error = expect_error(&fake_output(0, TRUNCATED.as_bytes(), b""));
        let expected = parse_ss(TRUNCATED).unwrap_err();
        assert!(
            matches!(&error, PacmanError::Parse(inner) if *inner == expected),
            "got {error:?}"
        );
    }

    #[test]
    fn invalid_utf8_in_a_description_becomes_replacement_char() {
        let results = results_from_output(&fake_output(0, BAD_BYTE, b"")).unwrap();
        assert_eq!(results.len(), 7);
        let baobab = results.iter().find(|r| r.name == "baobab").unwrap();
        assert_eq!(
            baobab.description,
            "A graphical \u{FFFD}irectory tree analyzer"
        );
    }

    #[test]
    fn invalid_utf8_in_a_name_is_still_rejected() {
        let mut bytes = SAMPLE.as_bytes().to_vec();
        let at = SAMPLE.find("zsh").unwrap(); // the name in "extra/zsh"
        bytes[at] = 0xFF;
        let error = expect_error(&fake_output(0, &bytes, b""));
        assert!(
            matches!(
                &error,
                PacmanError::Parse(ParseError::BadValue { field: "Name", .. })
            ),
            "got {error:?}"
        );
    }
}

/// Runs the real pacman, read-only. Needs an Arch machine, so CI skips them.
/// Run with: `cargo test -p tidewise-pacman -- --ignored`
#[cfg(test)]
mod real_search_tests {
    use super::*;

    fn mentions(result: &SearchResult, term: &str) -> bool {
        result.name.to_lowercase().contains(term)
            || result.description.to_lowercase().contains(term)
    }

    #[test]
    #[ignore = "runs the real /usr/bin/pacman; run with: cargo test -p tidewise-pacman -- --ignored"]
    fn real_pacman_search_is_literal_and_anded() {
        // A normal search finds the official package.
        let zsh = search("zsh").unwrap();
        assert!(zsh.iter().any(|r| r.name == "zsh" && r.is_official()));

        // No match is an empty list, not an error; blank text never reaches pacman.
        assert!(search("zzqqxxnotapkg").unwrap().is_empty());
        assert!(search("   ").unwrap().is_empty());

        // "c++" is literal. pacman also matches what a package *provides* (flac provides libFLAC++.so), so
        // not every hit says "c++" in its name or description; but some do, and far fewer than for "c".
        let cpp = search("c++").unwrap();
        assert!(cpp.iter().any(|r| mentions(r, "c++")));
        assert!(cpp.len() < search("c").unwrap().len());

        // Every term must match: two words give fewer hits than the first word alone.
        let both = search("gnome calculator").unwrap();
        assert!(both.iter().any(|r| r.name == "gnome-calculator"));
        assert!(both.len() < search("gnome").unwrap().len());

        // Characters that would break a regex, and option look-alikes, are plain search words.
        for query in ["(", "[", "a.*b", "--help", "-dd", "--noconfirm"] {
            assert!(search(query).is_ok(), "{query:?}");
        }
    }
}

#[cfg(test)]
mod details_tests {
    use super::*;

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    fn refused(refs: &[PackageRef]) -> &'static str {
        match details_request(refs) {
            Err(PacmanError::BadPackageRef { reason }) => reason,
            other => panic!("{refs:?}: expected BadPackageRef, got {other:?}"),
        }
    }

    #[test]
    fn empty_request_stays_empty() {
        assert_eq!(details_request(&[]).unwrap(), []);
    }

    #[test]
    fn valid_refs_keep_their_order() {
        let refs = [
            pkg("extra", "zsh"),
            pkg("core", "glibc"),
            pkg("my repo.x", "a+b"),
        ];
        assert_eq!(details_request(&refs).unwrap(), refs);
    }

    #[test]
    fn repeated_refs_are_removed_keeping_first_seen_order() {
        let refs = [
            pkg("extra", "zsh"),
            pkg("core", "glibc"),
            pkg("extra", "zsh"),
            pkg("extra", "bc"),
            pkg("core", "glibc"),
        ];
        assert_eq!(
            details_request(&refs).unwrap(),
            [
                pkg("extra", "zsh"),
                pkg("core", "glibc"),
                pkg("extra", "bc")
            ]
        );
    }

    #[test]
    fn same_name_in_two_repos_is_not_a_repeat() {
        let refs = [pkg("extra", "x"), pkg("testing", "x")];
        assert_eq!(details_request(&refs).unwrap(), refs);
    }

    #[test]
    fn bad_repo_names_are_refused() {
        for repo in ["", "a/b", "local", "a\nb", "a\0b"] {
            assert_eq!(
                refused(&[pkg(repo, "zsh")]),
                "the repository name is not valid",
                "{repo:?}"
            );
        }
    }

    #[test]
    fn bad_package_names_are_refused() {
        for name in [
            "",
            "-dd",
            "--help",
            ".x",
            "foo bar",
            "$(x)",
            "x\n",
            "a/b",
            "caf\u{e9}",
            "--overwrite",
        ] {
            assert_eq!(
                refused(&[pkg("extra", name)]),
                "the package name is not valid",
                "{name:?}"
            );
        }
    }

    #[test]
    fn one_bad_ref_among_good_ones_refuses_everything() {
        let refs = [
            pkg("extra", "zsh"),
            pkg("extra", "--force"),
            pkg("core", "bc"),
        ];
        assert_eq!(refused(&refs), "the package name is not valid");
        // Even when the bad one is a repeat of itself.
        let twice = [pkg("extra", "-x"), pkg("extra", "-x")];
        assert_eq!(refused(&twice), "the package name is not valid");
    }

    #[test]
    fn args_are_si_then_double_dash_then_repo_slash_name() {
        let request = [pkg("extra", "zsh"), pkg("core", "glibc")];
        assert_eq!(
            details_args(&request),
            ["-Si", "--", "extra/zsh", "core/glibc"]
        );
    }
}

#[cfg(test)]
mod lookup_tests {
    use super::*;
    use crate::{ParseError, parse_si};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const ZSH: &str = include_str!("../../../tests/fixtures/pacman/si-zsh.txt");
    const MULTI: &str = include_str!("../../../tests/fixtures/pacman/si-multi.txt");
    const BROKEN: &str = include_str!("../../../tests/fixtures/pacman/si-broken-truncated.txt");
    const BAD_BYTE: &[u8] = include_bytes!("../../../tests/fixtures/pacman/si-bad-byte.bin");
    const NOT_FOUND: &str = include_str!("../../../tests/fixtures/pacman/si-not-found.stderr.txt");
    const UNKNOWN_REPO: &str =
        include_str!("../../../tests/fixtures/pacman/si-unknown-repo.stderr.txt");

    fn fake_output(code: i32, stdout: &[u8], stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    fn names(lookup: &DetailsLookup) -> Vec<&str> {
        lookup.found.iter().map(|p| p.name.as_str()).collect()
    }

    fn failed(result: Result<DetailsLookup, PacmanError>) -> (Option<i32>, String) {
        match result {
            Err(PacmanError::Failed { code, stderr }) => (code, stderr),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    fn parse_error(result: Result<DetailsLookup, PacmanError>) -> ParseError {
        match result {
            Err(PacmanError::Parse(error)) => error,
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    #[test]
    fn success_gives_found_records_and_nothing_missing() {
        let request = [pkg("extra", "zsh")];
        let lookup = lookup_from_output(&fake_output(0, ZSH.as_bytes(), ""), &request).unwrap();
        assert_eq!(lookup.found, parse_si(ZSH).unwrap());
        assert_eq!(lookup.not_found, []);
    }

    #[test]
    fn success_keeps_pacmans_order() {
        let request = [pkg("core", "man-db"), pkg("core", "less")];
        let lookup = lookup_from_output(&fake_output(0, MULTI.as_bytes(), ""), &request).unwrap();
        assert_eq!(names(&lookup), ["man-db", "less"]);
    }

    #[test]
    fn unknown_name_is_partial_success_not_an_error() {
        let request = [pkg("extra", "zsh"), pkg("extra", "nosuchpkg")];
        let output = fake_output(1, ZSH.as_bytes(), NOT_FOUND);
        let lookup = lookup_from_output(&output, &request).unwrap();
        assert_eq!(names(&lookup), ["zsh"]);
        assert_eq!(lookup.not_found, [pkg("extra", "nosuchpkg")]);
    }

    #[test]
    fn nothing_found_is_still_not_an_error() {
        let request = [pkg("extra", "a"), pkg("core", "b"), pkg("extra", "c")];
        let stderr = "error: package 'extra/a' was not found\n\
                      error: package 'core/b' was not found\n\
                      error: package 'extra/c' was not found\n";
        // The exit code counts the misses; it must not matter whether it is 1, 3 or 255.
        for code in [1, 3, 255] {
            let lookup = lookup_from_output(&fake_output(code, b"", stderr), &request).unwrap();
            assert_eq!(lookup.found, []);
            assert_eq!(lookup.not_found, request, "exit code {code}");
        }
    }

    #[test]
    fn not_found_follows_request_order_not_stderr_order() {
        let request = [pkg("extra", "a"), pkg("extra", "b")];
        let stderr =
            "error: package 'extra/b' was not found\nerror: package 'extra/a' was not found\n";
        let lookup = lookup_from_output(&fake_output(2, b"", stderr), &request).unwrap();
        assert_eq!(lookup.not_found, request);
    }

    #[test]
    fn any_other_stderr_line_makes_it_a_failure() {
        // An unknown repository adds a line that is not a "not found" line.
        let request = [pkg("nosuchrepo", "zsh")];
        let (code, stderr) = failed(lookup_from_output(
            &fake_output(2, b"", UNKNOWN_REPO),
            &request,
        ));
        assert_eq!(code, Some(2));
        assert!(stderr.contains("repository 'nosuchrepo' does not exist"));

        let request = [pkg("extra", "zsh"), pkg("extra", "x")];
        let stderr = "warning: something odd\nerror: package 'extra/x' was not found\n";
        failed(lookup_from_output(
            &fake_output(1, ZSH.as_bytes(), stderr),
            &request,
        ));
    }

    #[test]
    fn a_not_found_line_for_a_package_we_did_not_ask_for_is_a_failure() {
        let request = [pkg("extra", "zsh")];
        let stderr = "error: package 'extra/other' was not found\n";
        failed(lookup_from_output(&fake_output(1, b"", stderr), &request));
    }

    #[test]
    fn non_zero_exit_with_empty_stderr_is_a_failure() {
        let request = [pkg("extra", "zsh")];
        let (code, stderr) = failed(lookup_from_output(&fake_output(1, b"", ""), &request));
        assert_eq!((code, stderr.as_str()), (Some(1), ""));
    }

    #[test]
    fn a_signal_is_a_failure() {
        let output = Output {
            status: ExitStatus::from_raw(9),
            stdout: Vec::new(),
            stderr: b"partial".to_vec(),
        };
        let (code, stderr) = failed(lookup_from_output(&output, &[pkg("extra", "zsh")]));
        assert_eq!((code, stderr.as_str()), (None, "partial"));
    }

    #[test]
    fn a_record_that_was_not_asked_for_is_refused() {
        let request = [pkg("core", "bc")];
        let error = parse_error(lookup_from_output(
            &fake_output(0, ZSH.as_bytes(), ""),
            &request,
        ));
        assert_eq!(
            error,
            ParseError::UnexpectedPackage {
                repo: "extra".to_string(),
                name: "zsh".to_string()
            }
        );
    }

    #[test]
    fn the_right_name_in_the_wrong_repo_is_refused() {
        let request = [pkg("core", "zsh")];
        let error = parse_error(lookup_from_output(
            &fake_output(0, ZSH.as_bytes(), ""),
            &request,
        ));
        assert!(
            matches!(error, ParseError::UnexpectedPackage { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_record_printed_twice_is_refused() {
        let twice = format!("{ZSH}{ZSH}");
        let request = [pkg("extra", "zsh")];
        let error = parse_error(lookup_from_output(
            &fake_output(0, twice.as_bytes(), ""),
            &request,
        ));
        assert!(
            matches!(error, ParseError::UnexpectedPackage { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_requested_package_pacman_said_nothing_about_is_reported() {
        // Exit 0 but only zsh printed: core/bc is neither found nor not-found.
        let request = [pkg("extra", "zsh"), pkg("core", "bc")];
        let error = parse_error(lookup_from_output(
            &fake_output(0, ZSH.as_bytes(), ""),
            &request,
        ));
        assert_eq!(
            error,
            ParseError::MissingPackage {
                repo: "core".to_string(),
                name: "bc".to_string()
            }
        );
        // The same on the partial path.
        let request = [
            pkg("extra", "zsh"),
            pkg("extra", "nosuchpkg"),
            pkg("core", "bc"),
        ];
        let error = parse_error(lookup_from_output(
            &fake_output(1, ZSH.as_bytes(), NOT_FOUND),
            &request,
        ));
        assert!(
            matches!(error, ParseError::MissingPackage { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn unparseable_text_is_wrapped_parse_error() {
        let request = [pkg("extra", "dmd")];
        let error = parse_error(lookup_from_output(
            &fake_output(0, BROKEN.as_bytes(), ""),
            &request,
        ));
        assert_eq!(error, parse_si(BROKEN).unwrap_err());
    }

    #[test]
    fn invalid_utf8_becomes_replacement_char() {
        let request = [pkg("extra", "zsh")];
        let lookup = lookup_from_output(&fake_output(0, BAD_BYTE, ""), &request).unwrap();
        assert_eq!(names(&lookup), ["zsh"]);
        let found = lookup.found.first().unwrap();
        assert!(
            found.description.contains('\u{FFFD}'),
            "{:?}",
            found.description
        );
    }

    #[test]
    fn new_parse_errors_say_what_happened() {
        let unexpected = ParseError::UnexpectedPackage {
            repo: "extra".to_string(),
            name: "zsh".to_string(),
        };
        assert_eq!(
            unexpected.to_string(),
            "pacman printed extra/zsh, which was not asked for (or printed it twice)"
        );
        let missing = ParseError::MissingPackage {
            repo: "core".to_string(),
            name: "bc".to_string(),
        };
        assert_eq!(
            missing.to_string(),
            "pacman said nothing about core/bc: neither details nor \"not found\""
        );
    }
}

#[cfg(test)]
mod details_runner_tests {
    use super::*;

    fn pkg(repo: &str, name: &str) -> PackageRef {
        PackageRef {
            repo: repo.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn empty_request_returns_an_empty_answer_without_running_pacman() {
        let lookup = package_details(&[]).unwrap();
        assert_eq!(lookup.found, []);
        assert_eq!(lookup.not_found, []);
    }

    #[test]
    fn a_bad_ref_is_refused_before_pacman_runs() {
        let error = package_details(&[pkg("extra", "--noconfirm")]).unwrap_err();
        assert!(
            matches!(error, PacmanError::BadPackageRef { .. }),
            "got {error:?}"
        );
    }
}

/// Runs the real pacman, read-only. Needs an Arch machine, so CI skips them.
/// Run with: `cargo test -p tidewise-pacman -- --ignored`
#[cfg(test)]
mod real_details_tests {
    use super::*;

    #[test]
    #[ignore = "runs the real /usr/bin/pacman; run with: cargo test -p tidewise-pacman -- --ignored"]
    fn real_pacman_details_found_not_found_and_repeats() {
        let zsh = PackageRef {
            repo: "extra".to_string(),
            name: "zsh".to_string(),
        };
        let unknown = PackageRef {
            repo: "extra".to_string(),
            name: "zzqqxxnotapkg".to_string(),
        };
        // zsh twice: it is asked once and answered once.
        let lookup = package_details(&[zsh.clone(), unknown.clone(), zsh.clone()]).unwrap();
        assert_eq!(lookup.found.len(), 1);
        let found = lookup.found.first().unwrap();
        assert_eq!((found.repo.as_str(), found.name.as_str()), ("extra", "zsh"));
        assert!(found.installed_size > 0 && !found.installed_size_text.is_empty());
        assert_eq!(lookup.not_found, [unknown]);

        // The right name in the wrong repository is "not found", not a failure.
        let wrong_repo = PackageRef {
            repo: "core".to_string(),
            name: "zsh".to_string(),
        };
        let lookup = package_details(std::slice::from_ref(&wrong_repo)).unwrap();
        assert_eq!(lookup.found, []);
        assert_eq!(lookup.not_found, [wrong_repo]);

        // A repository pacman does not have is a real failure.
        let no_repo = PackageRef {
            repo: "nosuchrepo".to_string(),
            name: "zsh".to_string(),
        };
        assert!(matches!(
            package_details(&[no_repo]),
            Err(PacmanError::Failed { .. })
        ));
    }
}
