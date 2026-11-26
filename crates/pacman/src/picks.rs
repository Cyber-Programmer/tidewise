//! Finding a provider pick: a name a dry run satisfied through `Provides` with a package nobody named. One copy of
//! the rule, used by the install resolver and by the root helper (golden rule 10, WBS 6.4a).
//!
//! Names are compared without versions: `parse_si` splits `java-runtime>=17` and `java-runtime=21` into a name and a
//! version rule, and only the names are matched here. Whether a *chosen* package really satisfies a version rule is
//! pacman's call, and the callers ask it with a dry run.

use crate::{Dependency, PackageDetails, PackageRef};

/// One name pacman answered itself: `dependency`, and the package it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPick {
    pub dependency: String,
    pub provider: PackageRef,
}

/// Shared libraries are provided by `soname` (`libz.so=1-64`): every package that links one would be a "provider
/// choice" otherwise. They are never a choice the user can make.
fn is_soname(name: &str) -> bool {
    name.ends_with(".so") || name.contains(".so.")
}

/// The names the dry run `list` satisfied through `Provides` with a package that was not `asked` for and is not
/// installed. A name in `answered` is skipped. The last provider in the list is reported: it is the one pacman took
/// (what it needs comes before it).
pub fn provider_picks<'a>(
    list: &[PackageRef],
    details_of: impl Fn(&PackageRef) -> Option<&'a PackageDetails>,
    asked: &[PackageRef],
    answered: &[&str],
    is_installed: impl Fn(&str) -> bool,
) -> Vec<ProviderPick> {
    let provides = |item: &PackageRef, name: &str| {
        details_of(item).is_some_and(|d| d.provides.iter().any(|p| p.name == name))
    };
    let mut found: Vec<ProviderPick> = Vec::new();
    for needer in list {
        let Some(needer_details) = details_of(needer) else {
            continue;
        };
        for dependency in &needer_details.depends_on {
            let name = &dependency.name;
            if is_soname(name)
                || answered.contains(&name.as_str())
                || found.iter().any(|pick| pick.dependency == *name)
            {
                continue;
            }
            let provider = list.iter().rev().find(|p| {
                p.name != needer.name
                    && p.name != *name
                    && !is_installed(&p.name)
                    && !asked.contains(p)
                    && provides(p, name)
            });
            if let Some(provider) = provider {
                found.push(ProviderPick {
                    dependency: name.clone(),
                    provider: provider.clone(),
                });
            }
        }
    }
    found
}

/// What the update's reading of golden rule 10 asks the repositories and the system. The app answers by running pacman
/// as the user, the helper by its fixed commands.
pub trait UpdateFacts {
    type Error;
    /// The repository packages that provide `name` (`-Ss '^name$'`, each confirmed with `-Si`).
    fn providers(&mut self, name: &str) -> Result<Vec<PackageRef>, Self::Error>;
    /// `pacman -T`: is `dependency` satisfied by what is installed now?
    fn satisfied(&mut self, dependency: &Dependency) -> Result<bool, Self::Error>;
}

/// The picks of `provider_picks` that are a choice in a system UPDATE (decision 0009). Stub.
pub fn update_picks<F: UpdateFacts>(
    picks: Vec<ProviderPick>,
    _listed: &[&PackageDetails],
    _is_installed: impl Fn(&str) -> bool,
    _facts: &mut F,
) -> Result<Vec<ProviderPick>, F::Error> {
    Ok(picks)
}

/// A dependency as `pacman -T` takes it (`name`, `name>=1.0`), or `None` if it is not plain. Stub.
pub fn dependency_text(_dependency: &Dependency) -> Option<String> {
    None
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

    // ---- the update's reading of the rule (decision 0009) -------------------------------------------------

    use crate::{Comparison, VersionRule};

    struct Facts {
        providers: Vec<&'static str>,
        satisfied: bool,
        asked: Vec<String>,
    }

    impl UpdateFacts for Facts {
        type Error = ();
        fn providers(&mut self, name: &str) -> Result<Vec<PackageRef>, ()> {
            self.asked.push(format!("providers {name}"));
            Ok(self.providers.iter().map(|n| plain(n)).collect())
        }
        fn satisfied(&mut self, dependency: &Dependency) -> Result<bool, ()> {
            self.asked.push(format!("satisfied {}", dependency.name));
            Ok(self.satisfied)
        }
    }

    fn plain(name: &str) -> PackageRef {
        PackageRef {
            repo: "extra".to_string(),
            name: name.to_string(),
        }
    }

    fn dependency(name: &str, rule: Option<(Comparison, &str)>) -> Dependency {
        Dependency {
            name: name.to_string(),
            rule: rule.map(|(comparison, version)| VersionRule {
                comparison,
                version: version.to_string(),
            }),
        }
    }

    /// A listed package that depends on `depends_on`.
    fn listed(name: &str, depends_on: Vec<Dependency>) -> PackageDetails {
        let mut details = parse_si(saved!("si-virtual-unnamed.txt"))
            .unwrap()
            .remove(0);
        details.name = name.to_string();
        details.depends_on = depends_on;
        details
    }

    /// The list pulls in `new` for `virtual`; the picks left after the update rule, and what was asked.
    fn left(
        rule: Option<(Comparison, &str)>,
        providers: &[&'static str],
        satisfied: bool,
        installed: &[&str],
        also_listed: &[&str],
    ) -> (usize, Vec<String>) {
        let mut details = vec![
            listed("app", vec![dependency("virtual", rule)]),
            listed("new", vec![]),
        ];
        details.extend(also_listed.iter().map(|name| listed(name, vec![])));
        let listed: Vec<&PackageDetails> = details.iter().collect();
        let mut facts = Facts {
            providers: providers.to_vec(),
            satisfied,
            asked: Vec::new(),
        };
        let picks = vec![ProviderPick {
            dependency: "virtual".to_string(),
            provider: plain("new"),
        }];
        let kept =
            update_picks(picks, &listed, |name| installed.contains(&name), &mut facts).unwrap();
        (kept.len(), facts.asked)
    }

    #[test]
    fn a_single_possible_provider_is_not_a_pick() {
        assert_eq!(left(None, &["new"], false, &[], &[]).0, 0);
        // The same package in two repositories is still one name.
        assert_eq!(left(None, &["new", "new"], false, &[], &[]).0, 0);
    }

    #[test]
    fn several_possible_providers_and_nothing_installed_is_a_pick() {
        assert_eq!(left(None, &["new", "other"], false, &[], &[]).0, 1);
    }

    #[test]
    fn no_provider_found_or_only_another_one_stays_a_pick() {
        assert_eq!(left(None, &[], false, &[], &[]).0, 1);
        assert_eq!(left(None, &["other"], false, &[], &[]).0, 1);
    }

    #[test]
    fn an_installed_provider_that_stays_is_not_a_pick() {
        assert_eq!(left(None, &["new", "other"], true, &["other"], &[]).0, 0);
        // Upgraded in the same list, and its new version still provides an unversioned name.
        assert_eq!(
            left(None, &["new", "other"], true, &["other"], &["other"]).0,
            0
        );
    }

    #[test]
    fn deptest_alone_is_not_enough() {
        // `-T` is satisfied now, but no installed package is among the new providers: the one that satisfies it
        // drops the name in this update (or it is not from the repositories), so pacman may ask.
        assert_eq!(
            left(None, &["new", "other"], true, &["gone"], &["gone"]).0,
            1
        );
        // And an installed provider in the repositories counts only if `-T` agrees (the installed version).
        assert_eq!(left(None, &["new", "other"], false, &["other"], &[]).0, 1);
    }

    #[test]
    fn a_versioned_need_is_not_trusted_to_a_provider_that_is_being_upgraded() {
        let rule = Some((Comparison::GreaterOrEqual, "2"));
        assert_eq!(left(rule, &["new", "other"], true, &["other"], &[]).0, 0);
        assert_eq!(
            left(rule, &["new", "other"], true, &["other"], &["other"]).0,
            1
        );
    }

    #[test]
    fn every_dependency_on_the_name_has_to_be_satisfied() {
        let details = [
            listed("app", vec![dependency("virtual", None)]),
            listed(
                "strict",
                vec![dependency("virtual", Some((Comparison::Equal, "3")))],
            ),
            listed("new", vec![]),
        ];
        let listed: Vec<&PackageDetails> = details.iter().collect();
        struct Only;
        impl UpdateFacts for Only {
            type Error = ();
            fn providers(&mut self, _: &str) -> Result<Vec<PackageRef>, ()> {
                Ok(vec![plain("new"), plain("other")])
            }
            fn satisfied(&mut self, dependency: &Dependency) -> Result<bool, ()> {
                Ok(dependency.rule.is_none())
            }
        }
        let picks = vec![ProviderPick {
            dependency: "virtual".to_string(),
            provider: plain("new"),
        }];
        let kept = update_picks(picks, &listed, |name| name == "other", &mut Only).unwrap();
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn a_lone_provider_needs_no_deptest_and_no_picks_ask_nothing() {
        assert_eq!(
            left(None, &["new"], false, &[], &[]).1,
            ["providers virtual"]
        );
        let mut facts = Facts {
            providers: vec![],
            satisfied: false,
            asked: Vec::new(),
        };
        assert!(
            update_picks(vec![], &[], |_| false, &mut facts)
                .unwrap()
                .is_empty()
        );
        assert!(facts.asked.is_empty());
    }

    #[test]
    fn a_dependency_is_written_the_way_pacman_takes_it_or_not_at_all() {
        let text = |name: &str, rule| dependency_text(&dependency(name, rule));
        assert_eq!(text("sh", None).as_deref(), Some("sh"));
        for (comparison, operator) in [
            (Comparison::Equal, "="),
            (Comparison::Less, "<"),
            (Comparison::LessOrEqual, "<="),
            (Comparison::Greater, ">"),
            (Comparison::GreaterOrEqual, ">="),
        ] {
            assert_eq!(
                text("java-runtime", Some((comparison, "1:17.0+9-1"))),
                Some(format!("java-runtime{operator}1:17.0+9-1"))
            );
        }
        for bad in ["-x", "a b", "", "a;b", "a\n"] {
            assert_eq!(text(bad, None), None, "{bad:?}");
        }
        for version in ["", "-1", "1 2", "1;2", "1$"] {
            assert_eq!(
                text("a", Some((Comparison::Equal, version))),
                None,
                "{version:?}"
            );
        }
    }
}
