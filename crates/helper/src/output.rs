//! Reading what pacman printed (threats T-06, T-07, T-09, T-10, T-15, T-21).
//!
//! Pure functions over text, tested on real saved output (`tests/fixtures/helper-jobs/`, captured in the test
//! container). Nothing here runs anything. pacman's output is outside data, so the parsers are strict and total: a line
//! that is not exactly the expected shape is an error, never a guess, and nothing from the text is copied into an error.

use std::collections::HashSet;

use tidewise_helper_protocol::{MAX_TRANSACTION, TransactionItem, is_valid_transaction_item};

/// Why pacman's output could not be used. A code only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputError {
    /// A line is not exactly `repo/name version` with a legal repository, name and version.
    Malformed,
    /// More lines than a transaction may have.
    TooMany,
}

/// The lines of `pacman -S|-Su --print --print-format '%r/%n %v'`, in the order pacman printed them.
pub fn parse_print(stdout: &str) -> Result<Vec<TransactionItem>, OutputError> {
    if stdout.is_empty() {
        return Ok(Vec::new());
    }
    let body = stdout.strip_suffix('\n').unwrap_or(stdout);
    let mut items = Vec::new();
    for line in body.split('\n') {
        if items.len() >= MAX_TRANSACTION {
            return Err(OutputError::TooMany);
        }
        items.push(parse_print_line(line)?);
    }
    Ok(items)
}

/// `repo/name version`, nothing else: one space, one slash, and the same allow-lists as a confirmed line.
fn parse_print_line(line: &str) -> Result<TransactionItem, OutputError> {
    let (spec, version) = line.split_once(' ').ok_or(OutputError::Malformed)?;
    let (repo, name) = spec.split_once('/').ok_or(OutputError::Malformed)?;
    let item = TransactionItem {
        repo: repo.to_string(),
        name: name.to_string(),
        version: version.to_string(),
    };
    if is_valid_transaction_item(&item) {
        Ok(item)
    } else {
        Err(OutputError::Malformed)
    }
}

/// True if the two lists hold the same lines, in any order (the same package at another version is a different
/// line). Pacman's order is the order of resolution, which the confirmed list also came from, but a different order
/// of the same packages is not a different transaction, so it is not refused.
pub fn same_transaction(confirmed: &[TransactionItem], found: &[TransactionItem]) -> bool {
    fn sorted(items: &[TransactionItem]) -> Vec<(&str, &str, &str)> {
        let mut keys: Vec<(&str, &str, &str)> = items
            .iter()
            .map(|item| {
                (
                    item.repo.as_str(),
                    item.name.as_str(),
                    item.version.as_str(),
                )
            })
            .collect();
        keys.sort_unstable();
        keys
    }
    sorted(confirmed) == sorted(found)
}

/// The three messages pacman prints when it cannot take the database lock (`-S`, `-D`, `-Sy`), as seen in the
/// container.
const LOCK_MESSAGES: [&str; 3] = [
    "error: failed to init transaction (unable to lock database)",
    "error: could not lock database",
    "error: failed to synchronize all databases (unable to lock database)",
];

/// True if pacman's error text says the database is locked. Only a line that starts with one of the three exact
/// messages counts (seen in the container): text of a package's own scriptlet that merely mentions it does not.
pub fn is_lock_failure(stderr: &str) -> bool {
    stderr.lines().any(|line| {
        LOCK_MESSAGES
            .iter()
            .any(|message| line.starts_with(message))
    })
}

/// The package names in the stdout of `pacman -Q` (lines `name version`), in order. Lines that are not
/// `name version` are ignored.
pub fn installed_names(query_stdout: &str) -> Vec<String> {
    query_pairs(query_stdout)
        .map(|(name, _)| name.to_string())
        .collect()
}

/// The names that are in the `pacman -Q` list taken before a run but not in the one taken after it, in the order of the
/// first list, each once. A version that changed is not a removal; a new package is not either. The caller must have
/// checked that both lists are complete (see `flows`): a cut-off list would look like a removal.
pub fn removed_names(before_stdout: &str, after_stdout: &str) -> Vec<String> {
    let still_there: HashSet<String> = installed_names(after_stdout).into_iter().collect();
    let mut seen = HashSet::new();
    installed_names(before_stdout)
        .into_iter()
        .filter(|name| !still_there.contains(name) && seen.insert(name.clone()))
        .collect()
}

/// The `name version` lines of a `pacman -Q` answer. Split on `\n` only: str::lines() would also accept `\r\n`, which
/// pacman never prints. A line is one name, one space and one version without white space.
fn query_pairs(query_stdout: &str) -> impl Iterator<Item = (&str, &str)> {
    query_stdout
        .split('\n')
        .filter_map(|line| line.split_once(' '))
        .filter(|(name, version)| {
            !name.is_empty() && !version.is_empty() && !version.chars().any(char::is_whitespace)
        })
}

/// How many items of `confirmed` are not installed at the confirmed version, judging by the stdout of
/// `pacman -Q` (lines `name version`; a name pacman does not know is only on stderr, so it is simply
/// missing from stdout). Lines that are not `name version` are ignored.
pub fn count_not_installed(confirmed: &[TransactionItem], query_stdout: &str) -> u32 {
    let installed: HashSet<(&str, &str)> = query_pairs(query_stdout).collect();
    let missing = confirmed
        .iter()
        .filter(|item| !installed.contains(&(item.name.as_str(), item.version.as_str())))
        .count();
    u32::try_from(missing).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRINT_INSTALL: &str =
        include_str!("../../../tests/fixtures/helper-jobs/print-install.out");
    const PRINT_UPDATE: &str =
        include_str!("../../../tests/fixtures/helper-jobs/print-update-replacement.out");
    const PRINT_NOTHING: &str =
        include_str!("../../../tests/fixtures/helper-jobs/print-nothing.out");
    const PRINT_NOT_FOUND_OUT: &str =
        include_str!("../../../tests/fixtures/helper-jobs/print-not-found.out");
    const LOCK_PRINT: &str =
        include_str!("../../../tests/fixtures/helper-jobs/lock-print-works.out");
    const VERIFY_PARTIAL_OUT: &str =
        include_str!("../../../tests/fixtures/helper-jobs/verify-partial.out");

    const LOCK_INSTALL_ERR: &str =
        include_str!("../../../tests/fixtures/helper-jobs/lock-install.err");
    const LOCK_SYNC_ERR: &str = include_str!("../../../tests/fixtures/helper-jobs/lock-sync.err");
    const LOCK_ASDEPS_ERR: &str =
        include_str!("../../../tests/fixtures/helper-jobs/lock-asdeps.err");
    const CONFLICT_ERR: &str = include_str!("../../../tests/fixtures/helper-jobs/conflict.err");
    const ASDEPS_MISSING_ERR: &str =
        include_str!("../../../tests/fixtures/helper-jobs/asdeps-missing.err");
    const PRINT_NOT_FOUND_ERR: &str =
        include_str!("../../../tests/fixtures/helper-jobs/print-not-found.err");
    const VERIFY_PARTIAL_ERR: &str =
        include_str!("../../../tests/fixtures/helper-jobs/verify-partial.err");

    fn item(repo: &str, name: &str, version: &str) -> TransactionItem {
        TransactionItem {
            repo: repo.to_string(),
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    // ---- parse_print on real output -----------------------------------------------------------------------

    #[test]
    fn the_real_dry_run_of_an_install_is_read_in_pacmans_order() {
        assert_eq!(
            parse_print(PRINT_INSTALL),
            Ok(vec![
                item("spike", "prov-b", "1-1"),
                item("spike", "needs-virtual", "1-1")
            ])
        );
    }

    #[test]
    fn the_real_dry_run_of_an_update_with_a_replacement_lists_the_new_package() {
        assert_eq!(
            parse_print(PRINT_UPDATE),
            Ok(vec![
                item("spike", "new-pkg", "1-1"),
                item("spike", "upd-pkg", "2-1")
            ])
        );
        // The removal of the replaced package is not in the output at all (measured): that is why --ask 66 exists.
        assert!(!PRINT_UPDATE.contains("old-pkg"));
    }

    #[test]
    fn a_dry_run_that_prints_nothing_is_an_empty_list() {
        assert_eq!(parse_print(PRINT_NOTHING), Ok(vec![]));
        assert_eq!(parse_print(PRINT_NOT_FOUND_OUT), Ok(vec![]));
        assert_eq!(parse_print(""), Ok(vec![]));
    }

    #[test]
    fn a_dry_run_made_while_the_lock_is_held_still_reads() {
        // --print needs no lock (measured), so the lock cannot be noticed this way.
        assert_eq!(
            parse_print(LOCK_PRINT),
            Ok(vec![item("spike", "plain-a", "1-1")])
        );
    }

    #[test]
    fn a_missing_last_new_line_is_fine() {
        assert_eq!(
            parse_print("extra/vim 9.1-1"),
            Ok(vec![item("extra", "vim", "9.1-1")])
        );
    }

    #[test]
    fn anything_that_is_not_exactly_a_line_is_refused() {
        for text in [
            "vim 1-1\n",
            "extra/vim\n",
            "extra/vim 1-1 x\n",
            "extra/vim  1-1\n",
            " extra/vim 1-1\n",
            "extra/vim 1-1 \n",
            "extra/vim\t1-1\n",
            "extra/vim 1-1\r\n",
            "extra/vim 1-1\n\nextra/git 2-1\n",
            "\nextra/vim 1-1\n",
            "extra/vim 1-1\nwarning: something\n",
            ":: Proceed with installation? [Y/n]\n",
            "extra/vim -1\n",
            "ex/tra/vim 1-1\n",
            "local/vim 1-1\n",
            "extra/--force 1-1\n",
            "extra/../x 1-1\n",
            "extra/caf\u{e9} 1-1\n",
            "extra/vim 1\u{0}1\n",
            "https://mirror/extra/vim-1-1.pkg.tar.zst\n",
        ] {
            assert_eq!(parse_print(text), Err(OutputError::Malformed), "{text:?}");
        }
    }

    #[test]
    fn more_lines_than_a_transaction_may_have_are_too_many() {
        let ok: String = (0..MAX_TRANSACTION)
            .map(|n| format!("extra/p{n} 1-1\n"))
            .collect();
        assert_eq!(
            parse_print(&ok).map(|items| items.len()),
            Ok(MAX_TRANSACTION)
        );
        let too_many = format!("{ok}extra/more 1-1\n");
        assert_eq!(parse_print(&too_many), Err(OutputError::TooMany));
    }

    #[test]
    fn nothing_removed_when_the_lists_are_the_same_or_only_versions_and_new_packages_differ() {
        let before = "a 1-1\nb 2-1\nc 3-1\n";
        assert_eq!(removed_names(before, before), Vec::<String>::new());
        assert_eq!(
            removed_names(before, "a 1-2\nb 2-9\nc 4-1\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            removed_names(before, "d 1-1\nc 3-1\nb 2-1\na 1-1\n"),
            Vec::<String>::new()
        );
        assert_eq!(removed_names("", "a 1-1\n"), Vec::<String>::new());
        assert_eq!(removed_names("", ""), Vec::<String>::new());
    }

    #[test]
    fn a_package_that_disappears_is_named_in_the_order_of_the_first_list() {
        assert_eq!(
            removed_names("a 1-1\nb 2-1\nc 3-1\n", "a 1-1\nc 3-1\n"),
            vec!["b"]
        );
        assert_eq!(
            removed_names("a 1-1\nb 2-1\nc 3-1\n", "b 2-1\n"),
            vec!["a", "c"]
        );
        assert_eq!(removed_names("a 1-1\nb 2-1\n", ""), vec!["a", "b"]);
    }

    #[test]
    fn a_name_listed_twice_before_is_one_removal() {
        assert_eq!(removed_names("a 1-1\na 1-1\nb 2-1\n", "b 2-1\n"), vec!["a"]);
    }

    #[test]
    fn noise_lines_in_either_list_are_not_packages() {
        let before = "error: x\na 1-1\nsomething else entirely\nb  2-1\n";
        let after = "error: y\na 1-1\n";
        assert_eq!(removed_names(before, after), Vec::<String>::new());
        assert_eq!(removed_names("a 1-1\nb 2-1\n", "noise\nb 2-1\n"), vec!["a"]);
    }

    #[test]
    fn two_big_lists_are_compared_without_trouble() {
        let before: String = (0..MAX_TRANSACTION)
            .map(|n| format!("p{n} 1-1\n"))
            .collect();
        let after: String = (2..MAX_TRANSACTION)
            .map(|n| format!("p{n} 1-2\n"))
            .collect();
        assert_eq!(removed_names(&before, &after), vec!["p0", "p1"]);
    }

    #[test]
    fn the_names_of_a_real_query_answer_are_read_in_order() {
        assert_eq!(
            installed_names(VERIFY_PARTIAL_OUT),
            vec!["upd-pkg", "old-pkg"]
        );
        assert_eq!(installed_names(""), Vec::<String>::new());
    }

    #[test]
    fn lines_that_are_not_name_and_version_give_no_name() {
        for noise in [
            "a\n",
            "a 1-1 extra\n",
            "error: package 'a' was not found\n",
            "a  1-1\n",
            " a 1-1\n",
            "a 1-1\r\n",
        ] {
            assert_eq!(installed_names(noise), Vec::<String>::new(), "{noise:?}");
        }
        assert_eq!(installed_names("noise\na 1-1\nb 2-1\n"), vec!["a", "b"]);
    }

    #[test]
    fn the_error_never_repeats_the_text() {
        let error = parse_print("SECRET-PATH/etc/shadow\n").unwrap_err();
        assert!(!format!("{error:?}").contains("SECRET"));
    }

    // ---- same_transaction ---------------------------------------------------------------------------------

    #[test]
    fn the_same_lines_in_any_order_are_the_same_transaction() {
        let a = vec![
            item("extra", "a", "1-1"),
            item("core", "b", "2-1"),
            item("extra", "c", "3-1"),
        ];
        let mut shuffled = a.clone();
        shuffled.reverse();
        assert!(same_transaction(&a, &a));
        assert!(same_transaction(&a, &shuffled));
        assert!(same_transaction(&[], &[]));
    }

    #[test]
    fn any_difference_is_a_different_transaction() {
        let a = vec![item("extra", "a", "1-1"), item("core", "b", "2-1")];
        let other_version = vec![item("extra", "a", "1-2"), item("core", "b", "2-1")];
        let other_repo = vec![item("multilib", "a", "1-1"), item("core", "b", "2-1")];
        let missing = vec![item("extra", "a", "1-1")];
        let extra = vec![
            item("extra", "a", "1-1"),
            item("core", "b", "2-1"),
            item("core", "z", "1-1"),
        ];
        let doubled = vec![item("extra", "a", "1-1"), item("extra", "a", "1-1")];
        let other_name = vec![item("extra", "a", "1-1"), item("core", "bb", "2-1")];
        for other in [
            other_version,
            other_repo,
            missing,
            extra,
            doubled,
            other_name,
        ] {
            assert!(!same_transaction(&a, &other), "{other:?}");
            assert!(!same_transaction(&other, &a), "{other:?}");
        }
        assert!(!same_transaction(&a, &[]));
        assert!(!same_transaction(&[], &a));
    }

    #[test]
    fn a_duplicated_line_is_not_the_same_as_one_line() {
        let once = vec![item("extra", "a", "1-1")];
        let twice = vec![item("extra", "a", "1-1"), item("extra", "a", "1-1")];
        assert!(!same_transaction(&once, &twice));
        assert!(same_transaction(&twice, &twice));
    }

    // ---- is_lock_failure ----------------------------------------------------------------------------------

    #[test]
    fn the_real_lock_messages_are_recognised() {
        for text in [LOCK_INSTALL_ERR, LOCK_SYNC_ERR, LOCK_ASDEPS_ERR] {
            assert!(is_lock_failure(text), "{text}");
        }
    }

    #[test]
    fn other_real_failures_are_not_lock_failures() {
        for text in [
            CONFLICT_ERR,
            ASDEPS_MISSING_ERR,
            PRINT_NOT_FOUND_ERR,
            VERIFY_PARTIAL_ERR,
            "",
        ] {
            assert!(!is_lock_failure(text), "{text}");
        }
    }

    #[test]
    fn only_a_line_that_starts_with_the_message_counts() {
        for text in [
            "scriptlet said: error: could not lock database: File exists\n",
            "  error: could not lock database: File exists\n",
            "ERROR: COULD NOT LOCK DATABASE\n",
            "error: could not lock\n",
            "unable to lock database\n",
            "error: failed to commit transaction (unable to lock database)\n",
        ] {
            assert!(!is_lock_failure(text), "{text:?}");
        }
        assert!(is_lock_failure(
            "warning: x\nerror: could not lock database: File exists\n  more\n"
        ));
    }

    // ---- count_not_installed ------------------------------------------------------------------------------

    #[test]
    fn a_declined_replacement_is_counted_on_the_real_output() {
        // Confirmed: new-pkg 1-1 and upd-pkg 2-1. After the run, pacman -Q knows upd-pkg 2-1 and old-pkg 1-1 only.
        let confirmed = parse_print(PRINT_UPDATE).unwrap();
        assert_eq!(count_not_installed(&confirmed, VERIFY_PARTIAL_OUT), 1);
    }

    #[test]
    fn everything_installed_at_the_confirmed_version_counts_zero() {
        let confirmed = vec![item("extra", "a", "1-1"), item("extra", "b", "2:3-4")];
        assert_eq!(count_not_installed(&confirmed, "b 2:3-4\na 1-1\n"), 0);
        assert_eq!(count_not_installed(&[], ""), 0);
    }

    #[test]
    fn another_version_is_not_installed_at_the_confirmed_version() {
        let confirmed = vec![item("extra", "a", "2-1"), item("extra", "b", "1-1")];
        assert_eq!(count_not_installed(&confirmed, "a 1-1\nb 1-1\n"), 1);
        assert_eq!(count_not_installed(&confirmed, "a 3-1\nb 1-1\n"), 1);
    }

    #[test]
    fn nothing_on_stdout_means_every_item_is_missing() {
        let confirmed = vec![
            item("extra", "a", "1-1"),
            item("extra", "b", "1-1"),
            item("core", "c", "1-1"),
        ];
        assert_eq!(count_not_installed(&confirmed, ""), 3);
    }

    #[test]
    fn lines_that_are_not_name_and_version_are_ignored() {
        let confirmed = vec![item("extra", "a", "1-1")];
        for noise in [
            "a\n",
            "a 1-1 extra\n",
            "error: package 'a' was not found\n",
            "a  1-1\n",
            " a 1-1\n",
            "a 1-1\r\n",
            "A 1-1\n",
            "aa 1-1\n",
        ] {
            assert_eq!(count_not_installed(&confirmed, noise), 1, "{noise:?}");
        }
        assert_eq!(
            count_not_installed(&confirmed, "noise\na 1-1\nmore noise\n"),
            0
        );
    }

    #[test]
    fn a_big_list_is_counted_without_trouble() {
        let confirmed: Vec<TransactionItem> = (0..MAX_TRANSACTION)
            .map(|n| item("extra", &format!("p{n}"), "1-1"))
            .collect();
        let half: String = (0..MAX_TRANSACTION / 2)
            .map(|n| format!("p{n} 1-1\n"))
            .collect();
        assert_eq!(
            count_not_installed(&confirmed, &half),
            u32::try_from(MAX_TRANSACTION - MAX_TRANSACTION / 2).unwrap()
        );
    }
}
