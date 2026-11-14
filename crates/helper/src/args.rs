//! Steps 4 and 5: what the caller put in argv and in the environment (threats T-02, T-04, T-01, T-14).
//!
//! argv carries exactly one thing: the summary text that `pkexec` shows in the password dialog. It is kept as raw
//! bytes, never turned into text, and used only for one comparison with the summary the helper recomputes. The
//! environment is not read at all, except `PKEXEC_UID`, which `pkexec` sets to the id of the user who called it
//! (`pkexec(1)`). That id goes into the log and nowhere else: it never allows or refuses anything.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

/// Why argv is not acceptable. A code only; nothing from argv is repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgsError {
    /// Not exactly the program name plus one argument.
    WrongCount,
}

/// The summary argument as raw bytes: argv must be exactly `[program, summary]`.
pub fn summary_arg(args: impl IntoIterator<Item = OsString>) -> Result<Vec<u8>, ArgsError> {
    let _ = args.into_iter().count();
    Err(ArgsError::WrongCount)
}

/// The caller's user id from `PKEXEC_UID`, or `None` if it is missing or is not a plain decimal number
/// (digits only, at most 10 of them, fitting in a `u32`). For the log only.
pub fn caller_uid(value: Option<OsString>) -> Option<u32> {
    let _ = value.map(OsStringExt::into_vec);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(text: &str) -> OsString {
        OsString::from(text)
    }

    fn argv(items: &[&str]) -> Vec<OsString> {
        items.iter().map(|item| os(item)).collect()
    }

    // ---- summary_arg ------------------------------------------------------------------------------------

    #[test]
    fn exactly_one_argument_after_the_program_name_is_the_summary() {
        let args = argv(&[
            "/usr/lib/tidewise/tidewise-helper",
            "update system (3 packages)",
        ]);
        assert_eq!(
            summary_arg(args),
            Ok(b"update system (3 packages)".to_vec())
        );
    }

    #[test]
    fn no_arguments_at_all_is_refused() {
        assert_eq!(summary_arg(Vec::new()), Err(ArgsError::WrongCount));
    }

    #[test]
    fn only_the_program_name_is_refused() {
        assert_eq!(summary_arg(argv(&["helper"])), Err(ArgsError::WrongCount));
    }

    #[test]
    fn two_or_more_arguments_are_refused() {
        for args in [
            argv(&["helper", "a", "b"]),
            argv(&["helper", "a", ""]),
            argv(&["helper", "--help", "update system (1 package)"]),
            argv(&["helper", "a", "b", "c", "d"]),
        ] {
            assert_eq!(summary_arg(args), Err(ArgsError::WrongCount));
        }
    }

    #[test]
    fn an_empty_summary_is_returned_as_it_is_the_comparison_refuses_it_later() {
        assert_eq!(summary_arg(argv(&["helper", ""])), Ok(Vec::new()));
    }

    #[test]
    fn an_argument_that_looks_like_an_option_is_just_bytes() {
        // It is never parsed as an option; the only use is the byte comparison.
        for text in ["--root=/x", "-U", "--", "--force", "-Syu"] {
            assert_eq!(
                summary_arg(argv(&["helper", text])),
                Ok(text.as_bytes().to_vec())
            );
        }
    }

    #[test]
    fn bytes_that_are_not_utf8_come_through_untouched() {
        let raw = vec![0xff, b'a', 0xc0, 0x80, 0x00, b'z'];
        let args = vec![os("helper"), OsString::from_vec(raw.clone())];
        assert_eq!(summary_arg(args), Ok(raw));
    }

    #[test]
    fn a_new_line_and_other_control_characters_are_kept_for_the_comparison_to_refuse() {
        let args = argv(&["helper", "update system (1 package)\nMESSAGE=x"]);
        assert_eq!(
            summary_arg(args),
            Ok(b"update system (1 package)\nMESSAGE=x".to_vec())
        );
    }

    // ---- caller_uid -------------------------------------------------------------------------------------

    #[test]
    fn a_plain_number_is_the_callers_uid() {
        assert_eq!(caller_uid(Some(os("1000"))), Some(1000));
        assert_eq!(caller_uid(Some(os("0"))), Some(0));
        assert_eq!(caller_uid(Some(os("4294967295"))), Some(4294967295));
    }

    #[test]
    fn a_missing_variable_is_unknown() {
        assert_eq!(caller_uid(None), None);
    }

    #[test]
    fn anything_that_is_not_a_plain_number_is_unknown() {
        for text in [
            "",
            " ",
            "0\nMESSAGE=x",
            "1000\n",
            "1000 ",
            " 1000",
            "+5",
            "-1",
            "1e3",
            "0x10",
            "10.0",
            "abc",
            "1000;id",
            "\u{ff11}\u{ff10}",
            "4294967296",
            "99999999999999999999",
            "00000000001",
        ] {
            assert_eq!(caller_uid(Some(os(text))), None, "{text:?}");
        }
    }

    #[test]
    fn a_value_that_is_not_utf8_is_unknown() {
        assert_eq!(caller_uid(Some(OsString::from_vec(vec![b'1', 0xff]))), None);
    }

    #[test]
    fn leading_zeros_within_ten_digits_read_as_the_number() {
        assert_eq!(caller_uid(Some(os("0007"))), Some(7));
    }

    #[test]
    fn a_huge_value_is_cut_off_by_the_length_rule_not_parsed() {
        let huge = "0".repeat(1_000_000);
        assert_eq!(caller_uid(Some(os(&huge))), None);
    }
}
