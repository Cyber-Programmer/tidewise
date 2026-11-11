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
}
