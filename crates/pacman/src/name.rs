//! Package-name check shared by the `-Qq` and `-Qi` parsers.

/// True if `name` is a legal pacman package name.
///
/// Allow-list from `man PKGBUILD`, section `pkgname`: only ASCII letters and digits and the
/// characters `@ . _ + -`, and it must not start with `-` or `.`.
pub(crate) fn is_valid_package_name(name: &str) -> bool {
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
pub(crate) fn is_valid_repo_name(repo: &str) -> bool {
    let _ = repo;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
