//! Finding a provider pick: a name a dry run satisfied through `Provides` with a package nobody named. One copy of
//! the rule, used by the install resolver and by the root helper (golden rule 10, WBS 6.4a).
//!
//! Package names are compared without versions: `parse_si` splits `java-runtime>=17` and `java-runtime=21` into a
//! name and a version rule, and only the names are matched here. Whether a *chosen* package really satisfies a version
//! rule is pacman's call, and the callers ask it with a dry run. Shared libraries are the exception: a soname need
//! (`libz.so=1-64`) is met only by a provider that carries the very same version.

use std::collections::{HashMap, HashSet};

use crate::{Comparison, Dependency, PackageDetails, PackageRef, VersionRule};
use tidewise_names::is_valid_package_name;

/// One name pacman answered itself: `dependency`, and the package it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPick {
    pub dependency: String,
    pub provider: PackageRef,
    /// For a shared library (`libz.so=1-64`): the version a provider must carry to count. `None` for other names.
    pub version: Option<VersionRule>,
}

/// Shared libraries are provided by `soname` (`libz.so=1-64`). Nearly every one has a single provider; the few with
/// several are choices pacman makes unasked (review 6.5 R-02).
pub fn is_soname(name: &str) -> bool {
    name.ends_with(".so") || name.contains(".so.")
}

/// Does `details` provide `name`, with exactly `version` when one is asked for?
pub(crate) fn provides_for(
    details: &PackageDetails,
    name: &str,
    version: Option<&VersionRule>,
) -> bool {
    details
        .provides
        .iter()
        .any(|p| p.name == name && version.is_none_or(|wanted| p.rule.as_ref() == Some(wanted)))
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
    let provides = |item: &PackageRef, name: &str, version: Option<&VersionRule>| {
        details_of(item).is_some_and(|d| provides_for(d, name, version))
    };
    let mut found: Vec<ProviderPick> = Vec::new();
    for needer in list {
        let Some(needer_details) = details_of(needer) else {
            continue;
        };
        for dependency in &needer_details.depends_on {
            let name = &dependency.name;
            // A soname need is its name AND version (`=1-32` and `=1-64` are two needs); other names go by name.
            let version = is_soname(name).then(|| dependency.rule.clone()).flatten();
            if answered.contains(&name.as_str())
                || found
                    .iter()
                    .any(|pick| pick.dependency == *name && pick.version == version)
                // A target that provides the library is what was asked for.
                || (is_soname(name)
                    && asked
                        .iter()
                        .any(|target| provides(target, name, version.as_ref())))
            {
                continue;
            }
            let provider = list.iter().rev().find(|p| {
                p.name != needer.name
                    && p.name != *name
                    && !is_installed(&p.name)
                    && !asked.contains(p)
                    && provides(p, name, version.as_ref())
            });
            if let Some(provider) = provider {
                found.push(ProviderPick {
                    dependency: name.clone(),
                    provider: provider.clone(),
                    version,
                });
            }
        }
    }
    found
}

/// Most names one search takes; more are split into several searches, none dropped.
pub const SEARCH_BATCH: usize = 100;

/// What judging whether a pick is a real choice asks the repositories and the system. The app answers by running
/// pacman as the user, the helper by its fixed commands.
pub trait ChoiceFacts {
    type Error;
    /// Every repository package that one search for these names finds (`-Ss '^(a|b)$'`), as `-Si` describes it.
    fn candidates(&mut self, names: &[String]) -> Result<Vec<PackageDetails>, Self::Error>;
    /// `pacman -T`: is `dependency` satisfied by what is installed now?
    fn satisfied(&mut self, dependency: &Dependency) -> Result<bool, Self::Error>;
}

/// The picks of `provider_picks` that are a choice in a system UPDATE (decision 0009). Installs keep naming every
/// pick; an update has no way to name one, so two cases where pacman has nothing to ask are let through:
///
/// - exactly one package in the repositories can provide the name;
/// - every listed dependency on the name is without a version, `pacman -T` says it is satisfied now, and an
///   installed package among the repository providers stays (not in the list, or upgraded to a version whose listed
///   record still provides the name).
///
/// `-T` alone is not enough: it answers for what is installed NOW, and a provider whose new version drops the name
/// leaves pacman with a choice (measured). What satisfies a name and is in no repository is not seen, so such a pick
/// stays (fails closed).
pub fn real_choices<F: ChoiceFacts>(
    picks: Vec<ProviderPick>,
    listed: &[&PackageDetails],
    is_installed: impl Fn(&str) -> bool,
    facts: &mut F,
) -> Result<Vec<ProviderPick>, F::Error> {
    if picks.is_empty() {
        return Ok(picks);
    }
    let mut names: Vec<String> = picks.iter().map(|pick| pick.dependency.clone()).collect();
    names.sort();
    names.dedup();
    // Many names are several searches: none is dropped, and none is a reason to refuse.
    let mut found = Vec::new();
    for batch in names.chunks(SEARCH_BATCH) {
        found.extend(facts.candidates(batch)?);
    }
    let mut kept = Vec::new();
    for pick in picks {
        let name = pick.dependency.as_str();
        let version = pick.version.as_ref();
        // Deduplicated: one search per batch may name a package more than once across batches.
        let mut providers: Vec<PackageRef> = Vec::new();
        for d in found.iter().filter(|d| provides_for(d, name, version)) {
            let package = PackageRef {
                repo: d.repo.clone(),
                name: d.name.clone(),
            };
            if !providers.contains(&package) {
                providers.push(package);
            }
        }
        // The same name in two repositories is two providers: pacman asks between them (R-07).
        let lone = !providers.is_empty() && providers.iter().all(|p| *p == pick.provider);
        if lone {
            continue;
        }
        // An installed provider that stays: not in the list, or upgraded to a version that still provides the need.
        // The new version is the LISTED record's, not any package of that name the search found (R-04).
        let stays = providers.iter().any(|p| {
            is_installed(&p.name)
                && listed
                    .iter()
                    .find(|d| d.name == p.name)
                    .is_none_or(|new| provides_for(new, name, version))
        });
        let mut still_satisfied = stays;
        for dependency in listed
            .iter()
            .flat_map(|d| &d.depends_on)
            .filter(|dependency| dependency.name == name)
            .filter(|dependency| version.is_none_or(|v| dependency.rule.as_ref() == Some(v)))
        {
            // Versions are compared only for a soname (the same one, above). Any other versioned need never passes
            // this way: the one that satisfies `-T` now may be another than the one that stays (R-03).
            let versioned_elsewhere = version.is_none() && dependency.rule.is_some();
            if !still_satisfied || versioned_elsewhere || !facts.satisfied(dependency)? {
                still_satisfied = false;
                break;
            }
        }
        if !still_satisfied {
            kept.push(pick);
        }
    }
    Ok(kept)
}

/// Strict rule C of "update system and install", for the app and the root helper (golden rule 10; the human's
/// decision after review 6.6 W-01).
///
/// A listed package is a choice nobody made when all of these hold:
/// - it is new (not installed) and the request does not name it (neither a target nor a chosen provider);
/// - it provides a name another listed package depends on (its own package name does not count);
/// - another repository package also provides that name, or is called it.
///
/// Names only: no version is compared, because pacman compares them in its own way (W-01). No dry run and no
/// `pacman -T`. It asks only the repository search behind `ChoiceFacts::candidates`.
///
/// Stricter than what pacman would ask: a package pulled in by its own name that also provides such a name is refused
/// too. Not seen: a provider outside the configured repositories, or in one whose `Usage` lacks `Search`.
pub fn unnamed_choices<F: ChoiceFacts>(
    listed: &[&PackageDetails],
    named: &[PackageRef],
    is_installed: impl Fn(&str) -> bool,
    facts: &mut F,
) -> Result<Vec<ProviderPick>, F::Error> {
    let mut needers: HashMap<&str, HashSet<&str>> = HashMap::new();
    for package in listed {
        for dependency in &package.depends_on {
            needers
                .entry(&dependency.name)
                .or_default()
                .insert(&package.name);
        }
    }
    // The new, unnamed packages that provide a name another listed package needs.
    let mut suspects: Vec<(&str, PackageRef)> = Vec::new();
    let mut seen: HashSet<(&str, &str)> = HashSet::new();
    for package in listed {
        let this = PackageRef {
            repo: package.repo.clone(),
            name: package.name.clone(),
        };
        if is_installed(&package.name) || named.contains(&this) {
            continue;
        }
        for provided in &package.provides {
            let name = provided.name.as_str();
            let needed = needers
                .get(name)
                .is_some_and(|who| who.iter().any(|other| *other != package.name));
            if name != package.name && needed && seen.insert((name, &package.name)) {
                suspects.push((name, this.clone()));
            }
        }
    }
    if suspects.is_empty() {
        return Ok(Vec::new());
    }

    let mut names: Vec<String> = suspects.iter().map(|(name, _)| name.to_string()).collect();
    names.sort();
    names.dedup();
    let mut found = Vec::new();
    for batch in names.chunks(SEARCH_BATCH) {
        found.extend(facts.candidates(batch)?);
    }
    Ok(suspects
        .into_iter()
        .filter(|(name, this)| {
            found.iter().any(|other| {
                (other.repo != this.repo || other.name != this.name)
                    && (other.name == *name || other.provides.iter().any(|p| p.name == *name))
            })
        })
        .map(|(name, provider)| ProviderPick {
            dependency: name.to_string(),
            provider,
            version: None,
        })
        .collect())
}

/// A dependency as `pacman -T` takes it (`name`, `name>=1.0`), or `None` if the name or the version is not plain.
/// It comes from pacman's own `-Si`, and is checked again before it is an argument.
pub fn dependency_text(dependency: &Dependency) -> Option<String> {
    if !is_valid_package_name(&dependency.name) {
        return None;
    }
    let Some(rule) = &dependency.rule else {
        return Some(dependency.name.clone());
    };
    let version = &rule.version;
    let plain = !version.is_empty()
        && !version.starts_with('-')
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '~' | ':' | '-'));
    let operator = match rule.comparison {
        Comparison::Equal => "=",
        Comparison::Less => "<",
        Comparison::LessOrEqual => "<=",
        Comparison::Greater => ">",
        Comparison::GreaterOrEqual => ">=",
    };
    plain.then(|| format!("{}{operator}{version}", dependency.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_si, parse_ss};

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
    fn a_shared_library_is_judged_by_its_exact_version() {
        // Review 6.5 R-02: pacman asks between two providers of a soname, so a soname is a name like any other...
        let pick = ("libz.so".to_string(), "zlib".to_string());
        assert_eq!(picks_for("libz.so=1-64", "zlib", "libz.so=1-64"), [pick]);
        // ...but only a provider of the same version counts (a lib32 package provides `=1-32`).
        assert_eq!(picks_for("libz.so=1-64", "zlib", "libz.so=1-32"), []);
        assert_eq!(picks_for("libz.so=1-64", "zlib", "libz.so"), []);
    }

    /// A target that provides the soname is what the user asked for: no pick, although another listed package
    /// provides it too.
    #[test]
    fn a_shared_library_a_target_provides_is_not_a_pick() {
        let si = record("app", "None", "libz.so=1-64")
            + &record("asked-lib", "libz.so=1-64", "None")
            + &record("other-lib", "libz.so=1-64", "None");
        let found = picks(
            &[named("asked-lib"), named("other-lib"), named("app")],
            &si,
            &[named("app"), named("asked-lib")],
            &[],
            &[],
        );
        assert_eq!(found, []);
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

    struct Facts {
        providers: Vec<&'static str>,
        satisfied: bool,
        asked: Vec<String>,
    }

    impl ChoiceFacts for Facts {
        type Error = ();
        fn candidates(&mut self, names: &[String]) -> Result<Vec<PackageDetails>, ()> {
            self.asked.push(format!("search {}", names.join(" ")));
            Ok(self
                .providers
                .iter()
                .map(|n| provider(n, "extra", names))
                .collect())
        }
        fn satisfied(&mut self, dependency: &Dependency) -> Result<bool, ()> {
            self.asked.push(format!("satisfied {}", dependency.name));
            Ok(self.satisfied)
        }
    }

    /// A repository package that provides every one of `names`.
    fn provider(name: &str, repo: &str, names: &[String]) -> PackageDetails {
        let mut details = listed(name, vec![]);
        details.repo = repo.to_string();
        details.provides = names.iter().map(|n| dependency(n, None)).collect();
        details
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
            version: None,
        }];
        let kept =
            real_choices(picks, &listed, |name| installed.contains(&name), &mut facts).unwrap();
        (kept.len(), facts.asked)
    }

    #[test]
    fn a_single_possible_provider_is_not_a_pick() {
        assert_eq!(left(None, &["new"], false, &[], &[]).0, 0);
    }

    /// Review 6.5 R-07, measured: pacman asks between the same name in two repositories.
    #[test]
    fn the_same_name_in_two_repositories_is_a_choice() {
        let details = [
            listed("app", vec![dependency("virtual", None)]),
            listed("new", vec![]),
        ];
        let listed: Vec<&PackageDetails> = details.iter().collect();
        struct TwoRepos;
        impl ChoiceFacts for TwoRepos {
            type Error = ();
            fn candidates(&mut self, names: &[String]) -> Result<Vec<PackageDetails>, ()> {
                Ok(vec![
                    provider("new", "extra", names),
                    provider("new", "extra-testing", names),
                ])
            }
            fn satisfied(&mut self, _: &Dependency) -> Result<bool, ()> {
                Ok(false)
            }
        }
        let picks = vec![ProviderPick {
            dependency: "virtual".to_string(),
            provider: plain("new"),
            version: None,
        }];
        let kept = real_choices(picks, &listed, |_| false, &mut TwoRepos).unwrap();
        assert_eq!(kept.len(), 1);
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
        assert_eq!(left_with_upgraded("virtual"), 0);
    }

    /// Review 6.5 R-04: the new version drops the name, while a package of the same name in another repository (so the
    /// search still lists it) provides it. Only the listed record counts.
    #[test]
    fn an_upgraded_provider_whose_new_version_drops_the_name_does_not_stay() {
        assert_eq!(left_with_upgraded("something-else"), 1);
    }

    /// `other` is installed, upgraded by the list to a version that provides `provides`; the picks left.
    fn left_with_upgraded(provides: &str) -> usize {
        let mut other = listed("other", vec![]);
        other.provides = vec![dependency(provides, None)];
        let details = [
            listed("app", vec![dependency("virtual", None)]),
            listed("new", vec![]),
            other,
        ];
        let listed: Vec<&PackageDetails> = details.iter().collect();
        let mut facts = Facts {
            providers: vec!["new", "other"],
            satisfied: true,
            asked: Vec::new(),
        };
        let picks = vec![ProviderPick {
            dependency: "virtual".to_string(),
            provider: plain("new"),
            version: None,
        }];
        real_choices(picks, &listed, |name| name == "other", &mut facts)
            .unwrap()
            .len()
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

    /// Review 6.5 R-03: an installed provider that stays may be too old for the need, while `-T` is satisfied only by
    /// the version that is about to drop the name. Versions are not compared here, so a versioned need never passes.
    #[test]
    fn a_versioned_need_never_passes_through_an_installed_provider() {
        let rule = Some((Comparison::GreaterOrEqual, "2"));
        assert_eq!(left(rule, &["new", "other"], true, &["other"], &[]).0, 1);
        // One possible provider is still no choice.
        assert_eq!(left(rule, &["new"], false, &[], &[]).0, 0);
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
        impl ChoiceFacts for Only {
            type Error = ();
            fn candidates(&mut self, names: &[String]) -> Result<Vec<PackageDetails>, ()> {
                Ok(vec![
                    provider("new", "extra", names),
                    provider("other", "extra", names),
                ])
            }
            fn satisfied(&mut self, dependency: &Dependency) -> Result<bool, ()> {
                Ok(dependency.rule.is_none())
            }
        }
        let picks = vec![ProviderPick {
            dependency: "virtual".to_string(),
            provider: plain("new"),
            version: None,
        }];
        let kept = real_choices(picks, &listed, |name| name == "other", &mut Only).unwrap();
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn a_lone_provider_needs_no_deptest_and_no_picks_ask_nothing() {
        assert_eq!(left(None, &["new"], false, &[], &[]).1, ["search virtual"]);
        let mut facts = Facts {
            providers: vec![],
            satisfied: false,
            asked: Vec::new(),
        };
        assert!(
            real_choices(vec![], &[], |_| false, &mut facts)
                .unwrap()
                .is_empty()
        );
        assert!(facts.asked.is_empty());
    }

    /// More names than one search takes: several searches, every name in one of them, none refused.
    #[test]
    fn many_names_are_searched_in_batches_and_none_is_dropped() {
        let count = SEARCH_BATCH * 2 + 3;
        let names: Vec<String> = (0..count).map(|n| format!("virtual{n}")).collect();
        let details = [
            listed("app", names.iter().map(|n| dependency(n, None)).collect()),
            listed("new", vec![]),
        ];
        let listed: Vec<&PackageDetails> = details.iter().collect();
        let picks: Vec<ProviderPick> = names
            .iter()
            .map(|name| ProviderPick {
                dependency: name.clone(),
                provider: plain("new"),
                version: None,
            })
            .collect();
        struct Batches(Vec<Vec<String>>);
        impl ChoiceFacts for Batches {
            type Error = ();
            fn candidates(&mut self, names: &[String]) -> Result<Vec<PackageDetails>, ()> {
                self.0.push(names.to_vec());
                Ok(vec![provider("new", "extra", names)])
            }
            fn satisfied(&mut self, _: &Dependency) -> Result<bool, ()> {
                Ok(false)
            }
        }
        let mut facts = Batches(Vec::new());
        let kept = real_choices(picks, &listed, |_| false, &mut facts).unwrap();
        // Each has the one provider its own batch found.
        assert!(kept.is_empty(), "{kept:?}");
        assert_eq!(facts.0.len(), 3);
        assert!(facts.0.iter().all(|batch| batch.len() <= SEARCH_BATCH));
        let mut searched: Vec<String> = facts.0.concat();
        searched.sort();
        let mut wanted = names.clone();
        wanted.sort();
        assert_eq!(searched, wanted);
    }

    /// The batched search must find exactly what one search per name finds (real pacman output; the container test
    /// compares them again on every run). `tidewise-single` is only PROVIDED, `libtide++.so` has plus signs.
    #[test]
    fn one_batched_search_finds_what_one_search_per_name_finds() {
        let found = |text: &str| -> Vec<(String, String)> {
            parse_ss(text)
                .unwrap()
                .into_iter()
                .map(|r| (r.repo, r.name))
                .collect()
        };
        let mut one_by_one: Vec<(String, String)> = [
            saved!("search-one-libtide.stdout.txt"),
            saved!("search-one-libtidewise.stdout.txt"),
            saved!("search-one-single.stdout.txt"),
            saved!("search-one-virtual.stdout.txt"),
            saved!("search-one-hello.stdout.txt"),
        ]
        .iter()
        .flat_map(|text| found(text))
        .collect();
        one_by_one.sort();
        let mut batched = found(saved!("search-batch.stdout.txt"));
        batched.sort();
        assert_eq!(batched, one_by_one);
        assert!(
            batched
                .iter()
                .any(|(_, name)| name == "tidewise-dummy-one-prov")
        );
        // The fixture was captured with exactly the pattern the code builds for these names.
        let names: Vec<String> = [
            "libtide++.so",
            "libtidewise.so",
            "tidewise-single",
            "tidewise-virtual",
            "tidewise-dummy-hello",
        ]
        .iter()
        .map(|n| n.to_string())
        .collect();
        assert_eq!(
            crate::exact_pattern_any(&names),
            r"^(libtide\+\+\.so|libtidewise\.so|tidewise-single|tidewise-virtual|tidewise-dummy-hello)$"
        );
    }

    fn soname(version: &str) -> Dependency {
        dependency("libx.so", Some((Comparison::Equal, version)))
    }

    /// `app` needs `libx.so=1-64` and the list brings `new` for it. `repository` is what the search finds (name and
    /// the version it provides); returns the picks left.
    fn soname_left(
        repository: &[(&str, &str)],
        satisfied: bool,
        installed: &[&str],
        upgraded: &[(&str, &str)],
    ) -> usize {
        struct Repo(Vec<PackageDetails>, bool);
        impl ChoiceFacts for Repo {
            type Error = ();
            fn candidates(&mut self, _: &[String]) -> Result<Vec<PackageDetails>, ()> {
                Ok(self.0.clone())
            }
            fn satisfied(&mut self, _: &Dependency) -> Result<bool, ()> {
                Ok(self.1)
            }
        }
        let found = repository
            .iter()
            .map(|(name, version)| {
                let mut details = listed(name, vec![]);
                details.repo = "extra".to_string();
                details.provides = vec![soname(version)];
                details
            })
            .collect();
        let mut details = vec![listed("app", vec![soname("1-64")]), listed("new", vec![])];
        for (name, version) in upgraded {
            let mut upgrade = listed(name, vec![]);
            upgrade.provides = vec![soname(version)];
            details.push(upgrade);
        }
        let listed: Vec<&PackageDetails> = details.iter().collect();
        let picks = vec![ProviderPick {
            dependency: "libx.so".to_string(),
            provider: plain("new"),
            version: soname("1-64").rule,
        }];
        real_choices(
            picks,
            &listed,
            |name| installed.contains(&name),
            &mut Repo(found, satisfied),
        )
        .unwrap()
        .len()
    }

    #[test]
    fn a_shared_library_is_a_choice_only_between_providers_of_the_same_version() {
        assert_eq!(soname_left(&[("new", "1-64")], false, &[], &[]), 0);
        assert_eq!(
            soname_left(&[("new", "1-64"), ("other", "1-32")], false, &[], &[]),
            0
        );
        assert_eq!(
            soname_left(&[("new", "1-64"), ("other", "1-64")], false, &[], &[]),
            1
        );
    }

    /// With the exact version compared, an installed provider that stays answers the need although it has a version.
    #[test]
    fn an_installed_provider_of_the_same_shared_library_is_not_a_choice() {
        let both = [("new", "1-64"), ("other", "1-64")];
        assert_eq!(soname_left(&both, true, &["other"], &[]), 0);
        // `-T` must agree, and an upgraded one must still carry that version.
        assert_eq!(soname_left(&both, false, &["other"], &[]), 1);
        assert_eq!(
            soname_left(&both, true, &["other"], &[("other", "1-64")]),
            0
        );
        assert_eq!(
            soname_left(&both, true, &["other"], &[("other", "2-64")]),
            1
        );
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

    /// A `-Si` record with only what the rule reads filled in.
    fn si(name: &str, version: &str, depends: &str) -> PackageDetails {
        providing(name, version, "None", depends)
    }

    fn providing(name: &str, version: &str, provides: &str, depends: &str) -> PackageDetails {
        let text = format!(
            "Repository      : extra\nName            : {name}\nVersion         : {version}\n\
             Description     : test\nArchitecture    : any\nURL             : https://example.invalid\n\
             Licenses        : GPL\nGroups          : None\nProvides        : {provides}\n\
             Depends On      : {depends}\nOptional Deps   : None\nConflicts With  : None\n\
             Replaces        : None\nDownload Size   : 1.00 KiB\nInstalled Size  : 1.00 KiB\n\
             Packager        : x\nBuild Date      : x\nValidated By    : None\n"
        );
        parse_si(&text).unwrap().remove(0)
    }

    fn extra(name: &str) -> PackageRef {
        PackageRef {
            repo: "extra".to_string(),
            name: name.to_string(),
        }
    }

    // ---- strict rule C: update and install (review 6.6 W-01) ----------------------------------------------------

    /// The repositories, as one search and `-Si` would show them. `pacman -T` must never be asked.
    struct Repository {
        packages: Vec<PackageDetails>,
        searched: Vec<Vec<String>>,
        broken: bool,
    }

    impl ChoiceFacts for Repository {
        type Error = &'static str;

        fn candidates(&mut self, names: &[String]) -> Result<Vec<PackageDetails>, &'static str> {
            self.searched.push(names.to_vec());
            if self.broken {
                return Err("the search failed");
            }
            Ok(self
                .packages
                .iter()
                .filter(|d| {
                    names.contains(&d.name) || d.provides.iter().any(|p| names.contains(&p.name))
                })
                .cloned()
                .collect())
        }

        fn satisfied(&mut self, _: &Dependency) -> Result<bool, &'static str> {
            panic!("rule C asks nothing about the installed system");
        }
    }

    /// What rule C finds, as `(name, package)`, for a transaction of `listed` against the repository of `listed`
    /// and `more`. `named` are the request's targets and chosen providers.
    fn choices(
        listed: &[PackageDetails],
        more: &[PackageDetails],
        named: &[&str],
        installed: &[&str],
    ) -> Vec<(String, String)> {
        let mut repository = Repository {
            packages: listed.iter().chain(more).cloned().collect(),
            searched: Vec::new(),
            broken: false,
        };
        let named: Vec<PackageRef> = named.iter().map(|name| extra(name)).collect();
        unnamed_choices(
            &listed.iter().collect::<Vec<_>>(),
            &named,
            |name| installed.contains(&name),
            &mut repository,
        )
        .unwrap()
        .into_iter()
        .map(|pick| (pick.dependency, pick.provider.name))
        .collect()
    }

    fn one(name: &str, package: &str) -> Vec<(String, String)> {
        vec![(name.to_string(), package.to_string())]
    }

    #[test]
    fn an_unnamed_new_package_providing_a_needed_name_with_another_provider_is_a_choice() {
        let listed = [
            si("app", "1-1", "virt"),
            providing("one", "1-1", "virt", "None"),
        ];
        let other = [providing("two", "1-1", "virt", "None")];
        assert_eq!(choices(&listed, &other, &["app"], &[]), one("virt", "one"));
        // Another listed package needs `one` by its name as well: still a choice (M4, V-01).
        let listed = [
            si("app", "1-1", "virt  one"),
            providing("one", "1-1", "virt", "None"),
        ];
        assert_eq!(choices(&listed, &other, &["app"], &[]), one("virt", "one"));
        // A package literally called by the name counts as another provider.
        let listed = [
            si("app", "1-1", "virt"),
            providing("one", "1-1", "virt", "None"),
        ];
        let literal = [si("virt", "1-1", "None")];
        assert_eq!(
            choices(&listed, &literal, &["app"], &[]),
            one("virt", "one")
        );
    }

    #[test]
    fn the_same_package_in_another_repository_counts_as_another_provider() {
        let listed = [
            si("app", "1-1", "virt"),
            providing("one", "1-1", "virt", "None"),
        ];
        let mut twin = providing("one", "1-1", "virt", "None");
        twin.repo = "testing".to_string();
        assert_eq!(choices(&listed, &[twin], &["app"], &[]), one("virt", "one"));
    }

    #[test]
    fn what_is_named_installed_lone_or_not_needed_is_no_choice() {
        let listed = [
            si("app", "1-1", "virt"),
            providing("one", "1-1", "virt", "None"),
        ];
        let other = [providing("two", "1-1", "virt", "None")];
        assert!(
            choices(&listed, &other, &["app", "one"], &[]).is_empty(),
            "named"
        );
        assert!(
            choices(&listed, &other, &["app"], &["one"]).is_empty(),
            "installed"
        );
        assert!(choices(&listed, &[], &["app"], &[]).is_empty(), "lone");
        // Nobody needs the name; or only the package that provides it does.
        let listed = [
            si("app", "1-1", "None"),
            providing("one", "1-1", "virt", "virt"),
        ];
        assert!(
            choices(&listed, &other, &["app"], &[]).is_empty(),
            "not needed"
        );
        // A need of the package's own name is not a choice, whoever else provides that name.
        let listed = [si("app", "1-1", "one"), si("one", "1-1", "None")];
        let rival = [providing("two", "1-1", "one", "None")];
        assert!(
            choices(&listed, &rival, &["app"], &[]).is_empty(),
            "by its own name"
        );
    }

    #[test]
    fn versions_are_not_looked_at() {
        // pacman compares versions where the texts differ (W-01): names only, and every one of these is a choice.
        for (need, first, second) in [
            ("libx.so>=1", "libx.so=1-64", "libx.so=1-64"),
            ("libx.so=1", "libx.so=1-64", "libx.so=1-64"),
            ("libx.so=1-64", "libx.so=1", "libx.so=1-64"),
            ("libx.so=1-64", "libx.so=1-64", "libx.so=1"),
            ("virt=1", "virt=1", "virt=99"),
        ] {
            let name = need.split(['>', '=']).next().unwrap();
            let listed = [
                si("app", "1-1", need),
                providing("one", "1-1", first, "None"),
            ];
            let other = [providing("two", "1-1", second, "None")];
            assert_eq!(
                choices(&listed, &other, &["app"], &[]),
                one(name, "one"),
                "{need} {first} {second}"
            );
        }
    }

    #[test]
    fn nothing_to_judge_searches_nothing_and_a_failed_search_is_an_error() {
        let listed = [si("app", "1-1", "None")];
        let refs: Vec<&PackageDetails> = listed.iter().collect();
        let mut repository = Repository {
            packages: Vec::new(),
            searched: Vec::new(),
            broken: false,
        };
        unnamed_choices(&refs, &[], |_| false, &mut repository).unwrap();
        assert!(repository.searched.is_empty());

        let listed = [
            si("app", "1-1", "virt"),
            providing("one", "1-1", "virt", "None"),
        ];
        repository.broken = true;
        assert_eq!(
            unnamed_choices(
                &listed.iter().collect::<Vec<_>>(),
                &[extra("app")],
                |_| false,
                &mut repository
            ),
            Err("the search failed")
        );
    }

    #[test]
    fn many_names_are_searched_in_batches() {
        // 250 unnamed providers, each of a name of its own that another package also provides.
        let mut listed = Vec::new();
        let mut other = Vec::new();
        let needs: Vec<String> = (0..250).map(|n| format!("virt{n}")).collect();
        listed.push(si("app", "1-1", &needs.join("  ")));
        for (n, name) in needs.iter().enumerate() {
            listed.push(providing(&format!("one{n}"), "1-1", name, "None"));
            other.push(providing(&format!("two{n}"), "1-1", name, "None"));
        }
        let mut repository = Repository {
            packages: listed.iter().chain(&other).cloned().collect(),
            searched: Vec::new(),
            broken: false,
        };
        let found = unnamed_choices(
            &listed.iter().collect::<Vec<_>>(),
            &[extra("app")],
            |_| false,
            &mut repository,
        )
        .unwrap();
        assert_eq!(found.len(), 250);
        let sizes: Vec<usize> = repository.searched.iter().map(Vec::len).collect();
        assert_eq!(sizes, [100, 100, 50]);
    }
}
