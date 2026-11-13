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
    let _ = (text, is_valid_package_name);
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
}
