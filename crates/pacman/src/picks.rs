//! Finding a provider pick: a name a dry run satisfied through `Provides` with a package nobody named. One copy of
//! the rule, used by the install resolver and by the root helper (golden rule 10, WBS 6.4a).
//!
//! Names are compared without versions: `parse_si` splits `java-runtime>=17` and `java-runtime=21` into a name and a
//! version rule, and only the names are matched here. Whether a *chosen* package really satisfies a version rule is
//! pacman's call, and the callers ask it with a dry run.

use crate::{PackageDetails, PackageRef};

/// One name pacman answered itself: `dependency`, and the package it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPick {
    pub dependency: String,
    pub provider: PackageRef,
}

/// The names the dry run `list` satisfied through `Provides` with a package that was not `asked` for and is not
/// installed. A name in `answered` is skipped. The last provider in the list is reported: it is the one pacman took
/// (what it needs comes before it).
pub fn provider_picks<'a>(
    _list: &[PackageRef],
    _details_of: impl Fn(&PackageRef) -> Option<&'a PackageDetails>,
    _asked: &[PackageRef],
    _answered: &[&str],
    _is_installed: impl Fn(&str) -> bool,
) -> Vec<ProviderPick> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_si;

    macro_rules! saved {
        ($name:literal) => {
            include_str!(concat!("../../../tests/fixtures/pacman/container/", $name))
        };
    }

    const REPO: &str = "tidewise-test";

    fn dummy(name: &str) -> PackageRef {
        PackageRef {
            repo: REPO.to_string(),
            name: format!("tidewise-dummy-{name}"),
        }
    }

    /// The helper's dry-run format, `repo/name version` (no size column, unlike the app's).
    fn list_of(print: &str) -> Vec<PackageRef> {
        print
            .lines()
            .map(|line| {
                let (spec, _) = line.split_once(' ').unwrap();
                let (repo, name) = spec.split_once('/').unwrap();
                PackageRef {
                    repo: repo.to_string(),
                    name: name.to_string(),
                }
            })
            .collect()
    }

    /// `(dependency, provider)` pairs, with `installed` names as the only installed packages.
    fn picks(
        list: &[PackageRef],
        si: &str,
        asked: &[PackageRef],
        answered: &[&str],
        installed: &[&str],
    ) -> Vec<(String, String)> {
        let details = parse_si(si).unwrap();
        provider_picks(
            list,
            |item| {
                details
                    .iter()
                    .find(|d| d.repo == item.repo && d.name == item.name)
            },
            asked,
            answered,
            |name| installed.contains(&name),
        )
        .into_iter()
        .map(|pick| (pick.dependency, pick.provider.name))
        .collect()
    }

    fn pair(dependency: &str, provider: &str) -> (String, String) {
        (dependency.to_string(), format!("tidewise-dummy-{provider}"))
    }

    #[test]
    fn each_unnamed_dry_run_has_one_pick() {
        for (print, si, target, expected) in [
            (
                saved!("print-virtual-unnamed.stdout.txt"),
                saved!("si-virtual-unnamed.txt"),
                "needs-virtual",
                pair("tidewise-virtual", "prov-a"),
            ),
            (
                saved!("print-vvirtual-unnamed.stdout.txt"),
                saved!("si-vvirtual-unnamed.txt"),
                "needs-vvirtual",
                pair("tidewise-vvirtual", "vprov-2"),
            ),
            (
                saved!("print-one-unnamed.stdout.txt"),
                saved!("si-one-unnamed.txt"),
                "needs-one",
                pair("tidewise-single", "one-prov"),
            ),
            (
                saved!("print-chain-unnamed.stdout.txt"),
                saved!("si-chain-unnamed.txt"),
                "needs-chain",
                pair("tidewise-chain", "chain-a"),
            ),
        ] {
            let found = picks(&list_of(print), si, &[dummy(target)], &[], &[]);
            assert_eq!(found, [expected], "{target}");
        }
    }

    #[test]
    fn a_provider_that_was_asked_for_is_not_a_pick() {
        let list = list_of(saved!("print-virtual-prov-b-named.stdout.txt"));
        let si = saved!("si-virtual-prov-b-named.txt");
        // Named as a provider or as a target makes no difference here: it was asked for by name.
        let asked = [dummy("needs-virtual"), dummy("prov-b")];
        assert_eq!(picks(&list, si, &asked, &[], &[]), []);
    }

    #[test]
    fn an_answered_name_is_skipped() {
        let list = list_of(saved!("print-virtual-unnamed.stdout.txt"));
        let si = saved!("si-virtual-unnamed.txt");
        let found = picks(
            &list,
            si,
            &[dummy("needs-virtual")],
            &["tidewise-virtual"],
            &[],
        );
        assert_eq!(found, []);
    }

    #[test]
    fn an_installed_provider_is_not_a_pick() {
        let list = list_of(saved!("print-virtual-unnamed.stdout.txt"));
        let si = saved!("si-virtual-unnamed.txt");
        let found = picks(
            &list,
            si,
            &[dummy("needs-virtual")],
            &[],
            &["tidewise-dummy-prov-a"],
        );
        assert_eq!(found, []);
    }

    // ---- hand-made records: the cases the dummy packages do not have ---------------------------------------------

    /// A `-Si` record with only what the rule reads filled in.
    fn record(name: &str, provides: &str, depends: &str) -> String {
        format!(
            "Repository      : {REPO}\nName            : {name}\nVersion         : 1.0-1\n\
             Description     : test\nArchitecture    : any\nURL             : https://example.invalid\n\
             Licenses        : GPL\nGroups          : None\nProvides        : {provides}\n\
             Depends On      : {depends}\nOptional Deps   : None\nConflicts With  : None\n\
             Replaces        : None\nDownload Size   : 1.00 KiB\nInstalled Size  : 1.00 KiB\n\
             Packager        : x\nBuild Date      : x\nValidated By    : None\n\n"
        )
    }

    fn named(name: &str) -> PackageRef {
        PackageRef {
            repo: REPO.to_string(),
            name: name.to_string(),
        }
    }

    /// The picks for `app` (asked) and `other`, with `app` depending on `depends`.
    fn picks_for(depends: &str, other: &str, provides: &str) -> Vec<(String, String)> {
        let si = record("app", "None", depends) + &record(other, provides, "None");
        picks(
            &[named(other), named("app")],
            &si,
            &[named("app")],
            &[],
            &[],
        )
    }

    #[test]
    fn a_version_rule_on_the_dependency_does_not_hide_the_pick() {
        let found = picks_for("java-runtime>=17", "jre", "java-runtime");
        assert_eq!(found, [("java-runtime".to_string(), "jre".to_string())]);
    }

    #[test]
    fn a_version_on_the_provides_entry_does_not_hide_the_pick() {
        let found = picks_for("java-runtime", "jre", "java-runtime=21");
        assert_eq!(found, [("java-runtime".to_string(), "jre".to_string())]);
    }

    #[test]
    fn a_soname_is_never_a_pick() {
        assert_eq!(picks_for("libz.so=1-64", "zlib", "libz.so=1-64"), []);
    }

    #[test]
    fn a_package_with_the_dependencys_own_name_is_not_a_pick() {
        assert_eq!(picks_for("jre", "jre", "jre"), []);
    }

    #[test]
    fn a_package_that_provides_its_own_dependency_is_not_a_pick() {
        let si = record("app", "thing", "thing");
        assert_eq!(picks(&[named("app")], &si, &[], &[], &[]), []);
    }
}
