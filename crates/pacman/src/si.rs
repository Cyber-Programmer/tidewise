//! Parser for `pacman -Si`: the details of packages in the repositories.

use crate::ParseError;
use crate::name::{is_valid_package_name, is_valid_repo_name};
use crate::record::{Record, split_records};
use crate::size::parse_size;

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

/// Everything `pacman -Si` says about one package in a repository.
///
/// "None" in pacman's text becomes an empty list or an empty string. Sizes come twice: in bytes (the
/// nearest whole byte to what pacman showed) and as pacman's own words, for the technical details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDetails {
    pub repo: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub url: String,
    pub licenses: Vec<String>,
    pub groups: Vec<String>,
    pub provides: Vec<Dependency>,
    pub depends_on: Vec<Dependency>,
    pub optional_deps: Vec<OptionalDependency>,
    pub conflicts_with: Vec<Dependency>,
    pub replaces: Vec<Dependency>,
    pub download_size: u64,
    /// pacman's own text, like `2.31 MiB`.
    pub download_size_text: String,
    pub installed_size: u64,
    /// pacman's own text, like `6.94 MiB`.
    pub installed_size_text: String,
}

/// Parses the output of `LC_ALL=C pacman -Si -- ...` into one entry per record, in pacman's order.
///
/// All-or-nothing: the first bad record makes the whole call an error naming the package and field.
pub fn parse_si(text: &str) -> Result<Vec<PackageDetails>, ParseError> {
    split_records(text)?.iter().map(build_details).collect()
}

/// Reads the fields we need out of one record, or says which one is wrong.
fn build_details(record: &Record) -> Result<PackageDetails, ParseError> {
    // Errors about the repository or the name can't name the package yet, like in `-Qi`.
    let repo = record.get("Repository").ok_or(ParseError::MissingField {
        package: None,
        field: "Repository",
    })?;
    if !is_valid_repo_name(repo) {
        return Err(ParseError::BadValue {
            package: None,
            field: "Repository",
            value: repo.to_string(),
        });
    }
    let name = record.get("Name").ok_or(ParseError::MissingField {
        package: None,
        field: "Name",
    })?;
    if !is_valid_package_name(name) {
        return Err(ParseError::BadValue {
            package: None,
            field: "Name",
            value: name.to_string(),
        });
    }
    let package = Some(name.to_string());

    let version = record.get("Version").ok_or(ParseError::MissingField {
        package: package.clone(),
        field: "Version",
    })?;
    if version.is_empty() {
        return Err(ParseError::BadValue {
            package,
            field: "Version",
            value: String::new(),
        });
    }
    let (download_size, download_size_text) = read_size(record, name, "Download Size")?;
    let (installed_size, installed_size_text) = read_size(record, name, "Installed Size")?;

    Ok(PackageDetails {
        repo: repo.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        description: text_value(record, "Description"),
        url: text_value(record, "URL"),
        // Licences are separated by two spaces; one licence can hold single spaces (`A OR B`).
        licenses: list_value(record, "Licenses")
            .split("  ")
            .map(str::trim)
            .filter(|licence| !licence.is_empty())
            .map(str::to_string)
            .collect(),
        groups: list_value(record, "Groups")
            .split_whitespace()
            .map(str::to_string)
            .collect(),
        provides: dependencies(record, name, "Provides")?,
        depends_on: dependencies(record, name, "Depends On")?,
        optional_deps: optional_dependencies(record, name)?,
        conflicts_with: dependencies(record, name, "Conflicts With")?,
        replaces: dependencies(record, name, "Replaces")?,
        download_size,
        download_size_text,
        installed_size,
        installed_size_text,
    })
}

/// A required size field: the bytes, and pacman's own text.
fn read_size(
    record: &Record,
    package: &str,
    field: &'static str,
) -> Result<(u64, String), ParseError> {
    let text = record.get(field).ok_or_else(|| ParseError::MissingField {
        package: Some(package.to_string()),
        field,
    })?;
    let bytes = parse_size(text).ok_or_else(|| ParseError::BadValue {
        package: Some(package.to_string()),
        field,
        value: text.to_string(),
    })?;
    Ok((bytes, text.to_string()))
}

/// The value of a list field, with "None" and a missing line both meaning "nothing".
fn list_value<'a>(record: &'a Record, field: &str) -> &'a str {
    match record.get(field) {
        Some("None") | None => "",
        Some(value) => value,
    }
}

/// The value of a text field; "None" and a missing line become an empty string.
fn text_value(record: &Record, field: &str) -> String {
    list_value(record, field).to_string()
}

/// A field made of dependencies separated by whitespace (`Depends On`, `Provides`, ...).
fn dependencies(
    record: &Record,
    package: &str,
    field: &'static str,
) -> Result<Vec<Dependency>, ParseError> {
    list_value(record, field)
        .split_whitespace()
        .map(|entry| {
            parse_dependency(entry).ok_or_else(|| ParseError::BadValue {
                package: Some(package.to_string()),
                field,
                value: entry.to_string(),
            })
        })
        .collect()
}

/// `Optional Deps`: one `name: reason` per line (continuation lines are already joined with `\n`).
fn optional_dependencies(
    record: &Record,
    package: &str,
) -> Result<Vec<OptionalDependency>, ParseError> {
    list_value(record, "Optional Deps")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            parse_optional_dep(line).ok_or_else(|| ParseError::BadValue {
                package: Some(package.to_string()),
                field: "Optional Deps",
                value: line.to_string(),
            })
        })
        .collect()
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
    // The separator is ": " (colon and space), so an epoch colon like `foo>=1:2.0` is left alone.
    // `name:` alone happens when the reason is empty and the reader trimmed the trailing space.
    let (dependency, reason) = line
        .split_once(": ")
        .or_else(|| line.strip_suffix(':').map(|name| (name, "")))
        .unwrap_or((line, ""));
    Some(OptionalDependency {
        dependency: parse_dependency(dependency)?,
        reason: reason.trim().to_string(),
    })
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

    const ZSH: &str = include_str!("../../../tests/fixtures/pacman/si-zsh.txt");
    const DMD: &str = include_str!("../../../tests/fixtures/pacman/si-dmd.txt");
    const FBDEV: &str = include_str!("../../../tests/fixtures/pacman/si-xf86-video-fbdev.txt");
    const IANA: &str = include_str!("../../../tests/fixtures/pacman/si-iana-etc.txt");
    const MULTI: &str = include_str!("../../../tests/fixtures/pacman/si-multi.txt");
    const BROKEN: &str = include_str!("../../../tests/fixtures/pacman/si-broken-truncated.txt");
    const BAD_BYTE: &[u8] = include_bytes!("../../../tests/fixtures/pacman/si-bad-byte.bin");

    /// Parses text that must hold exactly one record.
    fn one(text: &str) -> PackageDetails {
        let mut all = parse_si(text).unwrap_or_else(|e| panic!("parse failed: {e}"));
        assert_eq!(all.len(), 1, "expected exactly one record");
        all.remove(0)
    }

    fn plain(name: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
            rule: None,
        }
    }

    fn plains(names: &[&str]) -> Vec<Dependency> {
        names.iter().map(|n| plain(n)).collect()
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn missing(package: Option<&str>, field: &'static str) -> ParseError {
        ParseError::MissingField {
            package: package.map(str::to_string),
            field,
        }
    }

    fn bad_value(package: Option<&str>, field: &'static str, value: &str) -> ParseError {
        ParseError::BadValue {
            package: package.map(str::to_string),
            field,
            value: value.to_string(),
        }
    }

    #[test]
    fn si_zsh_every_field() {
        let zsh = one(ZSH);
        assert_eq!(zsh.repo, "extra");
        assert_eq!(zsh.name, "zsh");
        assert_eq!(zsh.version, "5.9.2-1");
        assert_eq!(
            zsh.description,
            "A very advanced and programmable command interpreter (shell) for UNIX"
        );
        assert_eq!(zsh.url, "https://www.zsh.org/");
        assert_eq!(zsh.licenses, strings(&["custom"]));
        assert_eq!(zsh.groups, Vec::<String>::new());
        assert_eq!(zsh.provides, Vec::<Dependency>::new());
        assert_eq!(zsh.depends_on, plains(&["pcre2", "libcap", "gdbm"]));
        assert_eq!(zsh.conflicts_with, Vec::<Dependency>::new());
        assert_eq!(zsh.replaces, Vec::<Dependency>::new());
        // 2.31 MiB = 2_422_210.56 bytes, 6.94 MiB = 7_277_117.44 bytes
        assert_eq!(zsh.download_size, 2_422_211);
        assert_eq!(zsh.download_size_text, "2.31 MiB");
        assert_eq!(zsh.installed_size, 7_277_117);
        assert_eq!(zsh.installed_size_text, "6.94 MiB");
    }

    #[test]
    fn si_optional_deps_continuation_lines_and_no_installed_marker() {
        let zsh = one(ZSH);
        let names: Vec<&str> = zsh
            .optional_deps
            .iter()
            .map(|o| o.dependency.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "grml-zsh-config",
                "zsh-autosuggestions",
                "zsh-completions",
                "zsh-doc",
                "zsh-history-substring-search",
                "zsh-lovers",
                "zsh-syntax-highlighting",
                "zshdb"
            ]
        );
        assert_eq!(zsh.optional_deps[0].reason, "grml's zsh setup");
        // `-Qi` marks installed ones with "[installed]"; `-Si` never does (checked on real pacman).
        assert!(zsh.optional_deps.iter().all(|o| !o.reason.contains('[')));
        assert_eq!(
            zsh.optional_deps[5].reason,
            "A collection of tips, tricks and examples for the Z shell."
        );
    }

    #[test]
    fn si_dmd_epoch_versioned_provides_and_two_groups() {
        let dmd = one(DMD);
        assert_eq!(dmd.version, "1:2.113.0-2");
        assert_eq!(dmd.groups, strings(&["dlang", "dlang-dmd"]));
        assert_eq!(
            dmd.provides,
            [Dependency {
                name: "d-compiler".to_string(),
                rule: Some(VersionRule {
                    comparison: Comparison::Equal,
                    version: "2.113.0".to_string()
                })
            }]
        );
        assert_eq!(dmd.optional_deps.len(), 3);
        assert_eq!(dmd.optional_deps[1].dependency.name, "gcc-multilib");
    }

    #[test]
    fn si_conflicts_keep_each_version_rule() {
        let fbdev = one(FBDEV);
        let rules: Vec<(&str, Comparison, &str)> = fbdev
            .conflicts_with
            .iter()
            .map(|d| {
                let rule = d.rule.as_ref().unwrap();
                (d.name.as_str(), rule.comparison, rule.version.as_str())
            })
            .collect();
        assert_eq!(
            rules,
            [
                ("xorg-server", Comparison::Less, "21.1.1"),
                ("X-ABI-VIDEODRV_VERSION", Comparison::Less, "25"),
                ("X-ABI-VIDEODRV_VERSION", Comparison::GreaterOrEqual, "26"),
            ]
        );
        assert_eq!(fbdev.groups, strings(&["xorg-drivers"]));
    }

    #[test]
    fn si_all_none_means_empty() {
        let iana = one(IANA);
        assert_eq!(iana.repo, "core");
        assert_eq!(iana.groups, Vec::<String>::new());
        assert_eq!(iana.provides, Vec::<Dependency>::new());
        assert_eq!(iana.depends_on, Vec::<Dependency>::new());
        assert_eq!(iana.optional_deps, Vec::<OptionalDependency>::new());
        assert_eq!(iana.conflicts_with, Vec::<Dependency>::new());
        assert_eq!(iana.replaces, Vec::<Dependency>::new());
        // A colon inside a value is fine.
        assert_eq!(iana.licenses, strings(&["custom:none"]));
        // 4114.47 KiB = 4_213_217.28 bytes; 401.79 KiB = 411_432.96 bytes
        assert_eq!(iana.installed_size, 4_213_217);
        assert_eq!(iana.installed_size_text, "4114.47 KiB");
        assert_eq!(iana.download_size, 411_433);
        assert_eq!(iana.download_size_text, "401.79 KiB");
    }

    #[test]
    fn si_none_in_text_fields_is_an_empty_string() {
        let text = ZSH.replace("https://www.zsh.org/", "None");
        assert_eq!(one(&text).url, "");
    }

    #[test]
    fn si_two_records_keep_pacmans_order() {
        let all = parse_si(MULTI).unwrap();
        let summary: Vec<(&str, &str, &str)> = all
            .iter()
            .map(|p| (p.repo.as_str(), p.name.as_str(), p.version.as_str()))
            .collect();
        assert_eq!(
            summary,
            [("core", "man-db", "2.13.1-2"), ("core", "less", "1:710-1")]
        );
        // A bare optional dependency has no reason.
        assert_eq!(all[0].optional_deps.len(), 1);
        assert_eq!(all[0].optional_deps[0].dependency, plain("gzip"));
        assert_eq!(all[0].optional_deps[0].reason, "");
    }

    #[test]
    fn si_licenses_are_split_on_two_spaces_not_one() {
        // `AND` / `OR` / `WITH` expressions contain single spaces and stay in one entry.
        let all = parse_si(MULTI).unwrap();
        assert_eq!(
            all[0].licenses,
            strings(&["GPL-2.0-or-later AND GPL-3.0-or-later"])
        );
        assert_eq!(
            all[1].licenses,
            strings(&["GPL-3.0-or-later OR LicenseRef-Less"])
        );
        let text = ZSH.replace(
            "custom\n",
            "GPL-3.0-or-later WITH GCC-exception-3.1  GFDL-1.3-or-later\n",
        );
        assert_eq!(
            one(&text).licenses,
            strings(&[
                "GPL-3.0-or-later WITH GCC-exception-3.1",
                "GFDL-1.3-or-later"
            ])
        );
    }

    #[test]
    fn si_missing_list_fields_are_empty() {
        let text = DMD
            .replace("Groups          : dlang  dlang-dmd\n", "")
            .replace("Provides        : d-compiler=2.113.0\n", "")
            .replace("Conflicts With  : None\n", "");
        let dmd = one(&text);
        assert_eq!(dmd.groups, Vec::<String>::new());
        assert_eq!(dmd.provides, Vec::<Dependency>::new());
        assert_eq!(dmd.conflicts_with, Vec::<Dependency>::new());
    }

    #[test]
    fn si_empty_input_is_no_packages() {
        assert_eq!(parse_si("").unwrap(), []);
        assert_eq!(parse_si("\n\n\n").unwrap(), []);
    }

    #[test]
    fn si_without_trailing_blank_line() {
        let with_blank = one(ZSH);
        assert_eq!(one(ZSH.trim_end_matches('\n')), with_blank);
    }

    #[test]
    fn si_broken_truncated_is_missing_field() {
        assert_eq!(parse_si(BROKEN), Err(missing(Some("dmd"), "Download Size")));
    }

    #[test]
    fn si_stray_non_utf8_byte_becomes_replacement_char() {
        // run.rs decodes lossily before parsing; so does this test.
        let text = String::from_utf8_lossy(BAD_BYTE);
        let zsh = one(&text);
        assert!(
            zsh.description.contains('\u{FFFD}'),
            "{:?}",
            zsh.description
        );
        assert!(!zsh.description.contains("programmable"));
        assert_eq!(zsh.name, "zsh");
    }

    #[test]
    fn si_missing_required_fields() {
        let cases = [
            ("Repository      : extra\n", None, "Repository"),
            ("Name            : zsh\n", None, "Name"),
            ("Version         : 5.9.2-1\n", Some("zsh"), "Version"),
            ("Download Size   : 2.31 MiB\n", Some("zsh"), "Download Size"),
            (
                "Installed Size  : 6.94 MiB\n",
                Some("zsh"),
                "Installed Size",
            ),
        ];
        for (line, package, field) in cases {
            let text = ZSH.replace(line, "");
            assert_ne!(text, ZSH, "{field}: line not found in the fixture");
            assert_eq!(parse_si(&text), Err(missing(package, field)), "{field}");
        }
    }

    #[test]
    fn si_bad_values() {
        let cases = [
            (
                "Repository      : extra",
                "Repository      : ",
                None,
                "Repository",
                "",
            ),
            (
                "Repository      : extra",
                "Repository      : a/b",
                None,
                "Repository",
                "a/b",
            ),
            (
                "Name            : zsh",
                "Name            : --force",
                None,
                "Name",
                "--force",
            ),
            (
                "Version         : 5.9.2-1",
                "Version         : ",
                Some("zsh"),
                "Version",
                "",
            ),
            (
                "Download Size   : 2.31 MiB",
                "Download Size   : lots",
                Some("zsh"),
                "Download Size",
                "lots",
            ),
            (
                "Installed Size  : 6.94 MiB",
                "Installed Size  : 6.94 TB",
                Some("zsh"),
                "Installed Size",
                "6.94 TB",
            ),
            (
                "Depends On      : pcre2  libcap  gdbm",
                "Depends On      : pcre2  --force",
                Some("zsh"),
                "Depends On",
                "--force",
            ),
            (
                "Depends On      : pcre2  libcap  gdbm",
                "Depends On      : pcre2>=",
                Some("zsh"),
                "Depends On",
                "pcre2>=",
            ),
            (
                "Provides        : None",
                "Provides        : a<=>b",
                Some("zsh"),
                "Provides",
                "a<=>b",
            ),
            (
                "Conflicts With  : None",
                "Conflicts With  : $(x)",
                Some("zsh"),
                "Conflicts With",
                "$(x)",
            ),
            (
                "Replaces        : None",
                "Replaces        : -x",
                Some("zsh"),
                "Replaces",
                "-x",
            ),
            (
                "zshdb: A debugger for zsh scripts",
                "--rm: A debugger",
                Some("zsh"),
                "Optional Deps",
                "--rm: A debugger",
            ),
        ];
        for (from, to, package, field, value) in cases {
            let text = ZSH.replace(from, to);
            assert_ne!(text, ZSH, "{field}: text to replace not found");
            assert_eq!(
                parse_si(&text),
                Err(bad_value(package, field, value)),
                "{field}"
            );
        }
    }

    #[test]
    fn si_duplicate_field_is_an_error() {
        let text = ZSH.replace("Licenses        : custom", "Version         : 9-9");
        assert_eq!(
            parse_si(&text),
            Err(ParseError::DuplicateField {
                package: Some("zsh".to_string()),
                field: "Version".to_string(),
            })
        );
    }

    #[test]
    fn si_one_bad_record_fails_all() {
        let text = MULTI.replace("Download Size   : 168.79 KiB", "Download Size   : lots");
        assert_eq!(
            parse_si(&text),
            Err(bad_value(Some("less"), "Download Size", "lots"))
        );
    }

    #[test]
    fn si_all_real_fixtures_parse() {
        for (file, text, count) in [
            ("si-zsh.txt", ZSH, 1),
            ("si-dmd.txt", DMD, 1),
            ("si-xf86-video-fbdev.txt", FBDEV, 1),
            ("si-iana-etc.txt", IANA, 1),
            ("si-multi.txt", MULTI, 2),
        ] {
            let all = parse_si(text).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(all.len(), count, "{file}");
        }
    }

    #[test]
    fn si_odd_inputs_never_panic() {
        let many_blank = "\n".repeat(10_000);
        let many_fields = "Name : a\n".repeat(10_000);
        let long_line = format!("Name : {}", "a".repeat(1_000_000));
        let many_deps = format!("Name : a\nDepends On : {}\n", "b ".repeat(100_000));
        let inputs = [
            "",
            ":",
            " : ",
            "Name",
            "Name :",
            "\t",
            "é : ü",
            "  continuation first",
            "Name : zsh\r\nVersion : 1\r\n",
            "Name : zsh\0\nVersion : 1\n",
            "Depends On : >=",
            "Optional Deps : :\n  : :\n",
            "Download Size : 99999999999999999999.00 GiB",
            "\u{feff}Name : zsh",
            many_blank.as_str(),
            many_fields.as_str(),
            long_line.as_str(),
            many_deps.as_str(),
        ];
        for input in inputs {
            // Ok or Err are both fine; the test only proves that it returns.
            let _ = parse_si(input);
        }
    }
}
