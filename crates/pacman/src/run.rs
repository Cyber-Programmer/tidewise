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
use crate::{InstalledPackage, PackageRef, PacmanError, SearchResult, parse_qi, parse_ss};

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
