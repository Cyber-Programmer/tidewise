//! Package and repository name rules, shared by `tidewise-pacman` (read side) and, later, the helper.
//!
//! One copy of the allow-lists, no dependencies, no I/O. Library code must not panic, so clippy forbids
//! `unwrap`, `expect`, indexing and `panic!` here (tests may still use them).
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]

/// True if `name` is a legal pacman package name.
///
/// Allow-list from `man PKGBUILD`, section `pkgname`: only ASCII letters and digits and the
/// characters `@ . _ + -`, and it must not start with `-` or `.`.
pub fn is_valid_package_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with(['-', '.']) {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '-'))
}

/// True if `repo` can be used as a repository name.
///
/// pacman's own rule (it comes from a `[section]` in pacman.conf, see the 4.3 notes): not empty, no `/`,
/// and not `local` (that name is the installed-packages database). On top of that we refuse control
/// characters, so nothing odd can reach the command line.
pub fn is_valid_repo_name(repo: &str) -> bool {
    !repo.is_empty()
        && !repo.contains('/')
        && repo != "local"
        && !repo.chars().any(char::is_control)
}

/// Longest repository name the helper accepts, in bytes.
pub const MAX_HELPER_REPO_NAME_BYTES: usize = 64;

/// True if `repo` may appear in a request to the helper: stricter than `is_valid_repo_name`.
///
/// The helper runs as root and puts repo names in the text pkexec shows, so it takes only the same ASCII set
/// as package names (letters, digits and `@ . _ + -`, not starting with `-` or `.`), at most 64 bytes, and
/// never `local`. That refuses look-alikes such as a Cyrillic "а" in `extrа`.
pub fn is_helper_repo_name(repo: &str) -> bool {
    repo.len() <= MAX_HELPER_REPO_NAME_BYTES && repo != "local" && is_valid_package_name(repo)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review 5.8 R-06. Every character above ASCII, one by one (about 1.1 million, a second or so): none may be part
    /// of a package or repository name, at the start or after a letter. This is what keeps look-alikes, zero-width and
    /// direction characters out of the confirm list, the pacman command line and the transaction compare.
    #[test]
    fn no_character_above_ascii_is_ever_a_name_character() {
        let mut name = String::new();
        for c in '\u{80}'..=char::MAX {
            for shape in [0, 1] {
                name.clear();
                if shape == 0 {
                    name.push('a');
                    name.push(c);
                } else {
                    name.push(c);
                    name.push('a');
                }
                assert!(
                    !is_valid_package_name(&name),
                    "{c:?} (U+{:04X}) in {name:?}",
                    c as u32
                );
                assert!(
                    !is_helper_repo_name(&name),
                    "{c:?} (U+{:04X}) in {name:?}",
                    c as u32
                );
            }
        }
    }

    #[test]
    fn accepts_real_package_names() {
        for name in [
            "zsh",
            "qt6-imageformats",
            "linux-api-headers",
            "lib32-glibc",
            "gtk+",
            "foo@1",
        ] {
            assert!(is_valid_package_name(name), "should accept {name:?}");
        }
    }

    #[test]
    fn rejects_everything_else() {
        for name in [
            "", "-foo", ".foo", "foo bar", "foo;rm", "foo/bar", "$(x)", "zsh\n", "café",
        ] {
            assert!(!is_valid_package_name(name), "should reject {name:?}");
        }
    }

    #[test]
    fn accepts_repo_names_pacman_accepts() {
        for repo in [
            "core",
            "extra",
            "multilib",
            "chaotic-aur",
            "my repo.x",
            "Ünï",
            "local2",
        ] {
            assert!(is_valid_repo_name(repo), "should accept {repo:?}");
        }
    }

    #[test]
    fn rejects_repo_names_pacman_or_we_refuse() {
        for repo in ["", "a/b", "/", "local", "a\nb", "a\0b", "\u{7f}", "x\ty"] {
            assert!(!is_valid_repo_name(repo), "should reject {repo:?}");
        }
    }

    #[test]
    fn helper_accepts_plain_ascii_repo_names() {
        for repo in [
            "core",
            "extra",
            "multilib",
            "chaotic-aur",
            "my_repo.x",
            "r+1",
            "a@b",
        ] {
            assert!(is_helper_repo_name(repo), "should accept {repo:?}");
        }
    }

    #[test]
    fn helper_refuses_everything_that_is_not_plain_ascii_or_is_reserved() {
        let too_long = "a".repeat(65);
        for repo in [
            "",
            "Ünï",
            "my repo.x",
            "extr\u{430}",
            "ｅｘｔｒａ",
            "local",
            "-x",
            ".x",
            "a/b",
            "a\0b",
            "a\nb",
            "a;b",
            too_long.as_str(),
        ] {
            assert!(!is_helper_repo_name(repo), "should refuse {repo:?}");
        }
        assert!(is_helper_repo_name(&"a".repeat(64)));
    }
}
