//! Which of the listed packages would replace an installed one (decision 0009, review 6.5 R-01). One rule for the app,
//! which blocks such an update before the password, and the root helper, which refuses it.
//!
//! Why every listed package counts, installed ones too: pacman lists an already installed replacer again, and the
//! real run (`--ask 66`) declines it and then upgrades the replaced package with whatever that needs, none of it
//! listed (measured).

use crate::{Dependency, PackageDetails, PackageRef, Replacement};

/// The replacements a dry run's list stands for: a listed package whose `Replaces` names an installed package, its own
/// name included. A versioned entry counts only when `satisfied` says the installed version fits (`pacman -T`); a caller
/// that cannot ask must say it does.
pub fn listed_replacements<E>(
    listed: &[&PackageDetails],
    is_installed: impl Fn(&str) -> bool,
    mut satisfied: impl FnMut(&Dependency) -> Result<bool, E>,
) -> Result<Vec<Replacement>, E> {
    let mut found = Vec::new();
    for package in listed {
        for old in &package.replaces {
            // pacman replaces by package name, never by a provided name.
            // Its own name too: pacman asks about that replacement like any other (review T-01).
            if !is_installed(&old.name) {
                continue;
            }
            if old.rule.is_some() && !satisfied(old)? {
                continue;
            }
            found.push(Replacement {
                new: PackageRef {
                    repo: package.repo.clone(),
                    name: package.name.clone(),
                },
                old: old.name.clone(),
            });
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_si;

    /// A `-Si` record with only what the rule reads filled in.
    fn record(name: &str, replaces: &str) -> PackageDetails {
        let text = format!(
            "Repository      : extra\nName            : {name}\nVersion         : 2-1\n\
             Description     : test\nArchitecture    : any\nURL             : https://example.invalid\n\
             Licenses        : GPL\nGroups          : None\nProvides        : None\nDepends On      : None\n\
             Optional Deps   : None\nConflicts With  : None\nReplaces        : {replaces}\n\
             Download Size   : 1.00 KiB\nInstalled Size  : 1.00 KiB\nPackager        : x\n\
             Build Date      : x\nValidated By    : None\n"
        );
        parse_si(&text).unwrap().remove(0)
    }

    /// `(new, old)` pairs, with `installed` installed and every version rule answered with `fits`; the rules asked.
    fn found(
        listed: &[PackageDetails],
        installed: &[&str],
        fits: bool,
    ) -> (Vec<(String, String)>, Vec<String>) {
        let listed: Vec<&PackageDetails> = listed.iter().collect();
        let mut asked = Vec::new();
        let found = listed_replacements::<()>(
            &listed,
            |name| installed.contains(&name),
            |dependency| {
                asked.push(dependency.name.clone());
                Ok(fits)
            },
        )
        .unwrap();
        (
            found.into_iter().map(|r| (r.new.name, r.old)).collect(),
            asked,
        )
    }

    fn pair(new: &str, old: &str) -> (String, String) {
        (new.to_string(), old.to_string())
    }

    #[test]
    fn a_listed_package_that_replaces_an_installed_one_is_a_replacement() {
        let listed = [record("new", "old  gone"), record("zsh", "None")];
        assert_eq!(found(&listed, &["old"], false).0, [pair("new", "old")]);
        // Installed or not, the replacer counts: an installed one is listed again.
        assert_eq!(
            found(&listed, &["old", "new"], false).0,
            [pair("new", "old")]
        );
        assert_eq!(found(&listed, &[], false).0, []);
    }

    /// Review T-01: pacman asks about a self-replacement too, and declining it can install what no list names.
    #[test]
    fn a_package_that_replaces_its_own_name_is_a_replacement_like_any_other() {
        let own = [record("old", "old  old-git")];
        assert_eq!(found(&own, &["old"], false).0, [pair("old", "old")]);
        assert_eq!(found(&own, &[], false).0, []);
        // A versioned self-replacement is asked about like any other.
        let versioned = [record("old", "old<1.0")];
        assert_eq!(found(&versioned, &["old"], false).0, []);
        assert_eq!(found(&versioned, &["old"], true).0, [pair("old", "old")]);
    }

    #[test]
    fn a_versioned_entry_counts_only_when_the_installed_version_fits() {
        let listed = [record("new", "old<1.0")];
        assert_eq!(
            found(&listed, &["old"], false),
            (vec![], vec!["old".to_string()])
        );
        assert_eq!(found(&listed, &["old"], true).0, [pair("new", "old")]);
        // Nothing to ask when the name is not installed.
        assert_eq!(found(&listed, &[], true), (vec![], vec![]));
    }

    #[test]
    fn a_version_that_cannot_be_asked_stops_the_rule() {
        let listed = [record("new", "old<1.0")];
        let listed: Vec<&PackageDetails> = listed.iter().collect();
        let result = listed_replacements(&listed, |_| true, |_| Err("no answer"));
        assert_eq!(result, Err("no answer"));
    }
}
