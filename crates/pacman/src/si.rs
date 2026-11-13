//! Parser for `pacman -Si`: the details of packages in the repositories.

use crate::name::is_valid_package_name;

/// How a dependency compares versions (`=`, `<`, `<=`, `>`, `>=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

/// The version part of a dependency: `>=` and `2.40` in `glibc>=2.40`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRule {
    pub comparison: Comparison,
    /// Exactly as pacman printed it (may have an epoch, like `1:2.0-1`).
    pub version: String,
}

/// An entry of `Depends On`, `Provides`, `Conflicts With`, `Replaces` or `Optional Deps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub rule: Option<VersionRule>,
}

/// Splits `glibc>=2.40` into a name and a version rule. `None` if the text is not a valid entry:
/// a bad name, or an operator with no usable version after it.
fn parse_dependency(text: &str) -> Option<Dependency> {
    // The name ends at the first `<`, `>` or `=`; names can't contain those, so this is unambiguous.
    let Some(split) = text.find(['<', '>', '=']) else {
        return is_valid_package_name(text).then(|| Dependency {
            name: text.to_string(),
            rule: None,
        });
    };
    let (name, rest) = text.split_at(split);
    if !is_valid_package_name(name) {
        return None;
    }
    // Two-character operators first, or `<=` would be read as `<` followed by a version `=...`.
    let (comparison, version) = if let Some(version) = rest.strip_prefix("<=") {
        (Comparison::LessOrEqual, version)
    } else if let Some(version) = rest.strip_prefix(">=") {
        (Comparison::GreaterOrEqual, version)
    } else if let Some(version) = rest.strip_prefix('<') {
        (Comparison::Less, version)
    } else if let Some(version) = rest.strip_prefix('>') {
        (Comparison::Greater, version)
    } else {
        (Comparison::Equal, rest.strip_prefix('=')?)
    };
    // The version is whatever follows, but it must be one plain word: no spaces, no control
    // characters, and no second operator (`foo<=>1`, `foo=1=2`).
    let usable = !version.is_empty()
        && !version
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '='));
    usable.then(|| Dependency {
        name: name.to_string(),
        rule: Some(VersionRule {
            comparison,
            version: version.to_string(),
        }),
    })
}

/// One line of `Optional Deps`: a dependency and the reason it helps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionalDependency {
    pub dependency: Dependency,
    /// Empty when pacman printed only the name.
    pub reason: String,
}

/// Reads `name: reason`, `name` or `name>=1.0: reason`. `None` if the dependency part is not valid.
fn parse_optional_dep(line: &str) -> Option<OptionalDependency> {
    let _ = line;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(name: &str, comparison: Comparison, version: &str) -> Option<Dependency> {
        Some(Dependency {
            name: name.to_string(),
            rule: Some(VersionRule {
                comparison,
                version: version.to_string(),
            }),
        })
    }

    #[test]
    fn dependency_without_a_rule() {
        assert_eq!(
            parse_dependency("glibc"),
            Some(Dependency {
                name: "glibc".to_string(),
                rule: None
            })
        );
    }

    #[test]
    fn dependency_with_each_operator() {
        assert_eq!(parse_dependency("a=1"), dep("a", Comparison::Equal, "1"));
        assert_eq!(parse_dependency("a<1"), dep("a", Comparison::Less, "1"));
        assert_eq!(
            parse_dependency("a<=1"),
            dep("a", Comparison::LessOrEqual, "1")
        );
        assert_eq!(parse_dependency("a>1"), dep("a", Comparison::Greater, "1"));
        assert_eq!(
            parse_dependency("a>=1"),
            dep("a", Comparison::GreaterOrEqual, "1")
        );
    }

    #[test]
    fn dependency_real_examples_from_pacman() {
        assert_eq!(
            parse_dependency("linux-api-headers>=4.10"),
            dep("linux-api-headers", Comparison::GreaterOrEqual, "4.10")
        );
        assert_eq!(
            parse_dependency("d-compiler=2.113.0"),
            dep("d-compiler", Comparison::Equal, "2.113.0")
        );
        assert_eq!(
            parse_dependency("X-ABI-VIDEODRV_VERSION>=26"),
            dep("X-ABI-VIDEODRV_VERSION", Comparison::GreaterOrEqual, "26")
        );
    }

    #[test]
    fn dependency_version_keeps_epoch_and_release() {
        assert_eq!(
            parse_dependency("foo>=1:2.0-1"),
            dep("foo", Comparison::GreaterOrEqual, "1:2.0-1")
        );
    }

    #[test]
    fn dependency_with_a_bad_name_is_refused() {
        for text in [
            "", "--force", "-x>=1", ".hidden", ">=1", "foo bar", "foo;rm", "café>=1", "a/b",
        ] {
            assert_eq!(parse_dependency(text), None, "should refuse {text:?}");
        }
    }

    #[test]
    fn dependency_with_a_bad_version_is_refused() {
        for text in [
            "foo=", "foo<=", "foo<=>1", "foo=1=2", "foo>= 1", "foo>=1 2", "foo>=1\n", "foo>=1\0",
        ] {
            assert_eq!(parse_dependency(text), None, "should refuse {text:?}");
        }
    }

    fn optional(name: &str, reason: &str) -> Option<OptionalDependency> {
        Some(OptionalDependency {
            dependency: Dependency {
                name: name.to_string(),
                rule: None,
            },
            reason: reason.to_string(),
        })
    }

    #[test]
    fn optional_dep_name_and_reason() {
        assert_eq!(
            parse_optional_dep("grml-zsh-config: grml's zsh setup"),
            optional("grml-zsh-config", "grml's zsh setup")
        );
    }

    #[test]
    fn optional_dep_bare_name_has_empty_reason() {
        assert_eq!(parse_optional_dep("gzip"), optional("gzip", ""));
    }

    #[test]
    fn optional_dep_reason_may_contain_colons() {
        assert_eq!(
            parse_optional_dep("a: Plugins: TIFF, MNG"),
            optional("a", "Plugins: TIFF, MNG")
        );
    }

    #[test]
    fn optional_dep_name_followed_by_a_lone_colon_has_empty_reason() {
        // The record reader trims trailing spaces, so `name: ` arrives as `name:`.
        assert_eq!(parse_optional_dep("foo:"), optional("foo", ""));
    }

    #[test]
    fn optional_dep_with_a_version_rule() {
        let parsed = parse_optional_dep("python>=3: For Python bindings").unwrap();
        assert_eq!(parsed.reason, "For Python bindings");
        assert_eq!(
            parsed.dependency,
            dep("python", Comparison::GreaterOrEqual, "3").unwrap()
        );
    }

    #[test]
    fn optional_dep_epoch_colon_is_not_the_reason_separator() {
        let parsed = parse_optional_dep("foo>=1:2.0: reason").unwrap();
        assert_eq!(parsed.reason, "reason");
        assert_eq!(
            parsed.dependency,
            dep("foo", Comparison::GreaterOrEqual, "1:2.0").unwrap()
        );
    }

    #[test]
    fn optional_dep_with_a_bad_name_is_refused() {
        for line in [
            "",
            ": reason",
            "--force: x",
            "foo bar: x",
            "foo;rm",
            "a/b: x",
        ] {
            assert_eq!(parse_optional_dep(line), None, "should refuse {line:?}");
        }
    }
}
