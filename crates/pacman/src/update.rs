//! The review list of "Update system" (WBS 6.5): what `pacman -Su` would do, worked out from four reads of the same
//! package lists: `-Qu`, the dry run (`-Su --print`), `-Si` of what it lists and `-Qi` of everything installed. Pure:
//! text in, a plan out. The facts it rests on are measured in `tests/container/tests/pacman_update.rs` (decision 0009).
//!
//! What the dry run does not say, and this works out:
//! - a replacement in it is declined by the real run, which may then upgrade the replaced package and what that
//!   needs, none of it listed: such an update is blocked (`listed_replacements`, filled in by `update_lookup`);
//! - it says nothing about a conflict, and the real run then aborts the whole update;
//! - it takes the first provider of a virtual name without saying so (golden rule 10: the helper refuses that).

use std::collections::HashMap;

use crate::record::split_records;
use crate::si::dependencies;
use crate::{
    Dependency, InstalledPackage, PackageDetails, PackageRef, ParseError, PendingUpdate, PlanItem,
    PrintFailure, ProviderPick, Role, parse_print, parse_print_failure, parse_qi, parse_qu,
    parse_si, provider_picks,
};

/// A package that would replace an installed one. Tidewise does not do replacements yet: the update is blocked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub new: PackageRef,
    pub old: String,
}

/// A listed package and an installed one that cannot both be there. The real run would abort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub package: String,
    pub installed: String,
}

/// Why the dry run failed: `package` needs a newer `held`, which `IgnorePkg` holds back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldBlock {
    pub package: String,
    pub held: String,
}

/// What `pacman -Su` would do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePlan {
    /// Every line of the dry run, in pacman's order: the list the helper compares.
    pub items: Vec<PlanItem>,
    /// Not known to `update_plan`: a versioned `Replaces` needs pacman to judge (`listed_replacements`).
    pub replacements: Vec<Replacement>,
    /// `[ignored]` updates: held back by `IgnorePkg` / `IgnoreGroup`.
    pub held_back: Vec<PendingUpdate>,
    pub conflicts: Vec<Conflict>,
    /// Names pacman would answer itself with a new package (the helper refuses these).
    pub picks: Vec<ProviderPick>,
}

impl UpdatePlan {
    /// Bytes still to download.
    pub fn download_total(&self) -> u64 {
        self.items.iter().map(|item| item.download_size).sum()
    }

    /// How much more (or less) disk space the new versions take, in bytes.
    pub fn size_change(&self) -> i64 {
        self.items
            .iter()
            .map(|item| {
                let new = i64::try_from(item.installed_size).unwrap_or(i64::MAX);
                let old = i64::try_from(item.current_size.unwrap_or(0)).unwrap_or(i64::MAX);
                new.saturating_sub(old)
            })
            .fold(0, i64::saturating_add)
    }
}

/// The plan, from `-Qu`, the dry run, `-Si` of the printed packages and `-Qi` of everything installed.
pub fn update_plan(qu: &str, print: &str, si: &str, qi: &str) -> Result<UpdatePlan, ParseError> {
    let held_back = parse_qu(qu)?
        .into_iter()
        .filter(|update| update.ignored)
        .collect();
    let printed = parse_print(print)?;
    let details: HashMap<PackageRef, PackageDetails> = parse_si(si)?
        .into_iter()
        .map(|d| (package_ref(&d.repo, &d.name), d))
        .collect();
    let installed: HashMap<String, InstalledPackage> = parse_qi(qi)?
        .into_iter()
        .map(|p| (p.name.clone(), p))
        .collect();
    let relations = installed_relations(qi)?;

    let mut listed: Vec<Listed> = Vec::new();
    for line in &printed {
        let package = package_ref(&line.repo, &line.name);
        let details = details
            .get(&package)
            .ok_or_else(|| ParseError::MissingPackage {
                repo: line.repo.clone(),
                name: line.name.clone(),
            })?;
        let current = installed.get(&line.name);
        listed.push(Listed {
            item: PlanItem {
                package,
                version: line.version.clone(),
                role: if current.is_some() {
                    Role::Update
                } else {
                    Role::Dependency
                },
                installed_version: current.map(|p| p.version.clone()),
                download_size: line.download_size,
                installed_size: details.installed_size,
                installed_size_text: details.installed_size_text.clone(),
                current_size: current.map(|p| p.installed_size),
            },
            details,
        });
    }

    let conflicts = conflicts_of(&listed.iter().collect::<Vec<_>>(), &relations);

    // The whole list, as the helper sees it: it checks every line it is sent.
    let refs: Vec<PackageRef> = listed.iter().map(|l| l.item.package.clone()).collect();
    let picks = provider_picks(
        &refs,
        |item| details.get(item),
        &[],
        &[],
        |name| installed.contains_key(name),
    );

    Ok(UpdatePlan {
        items: listed.into_iter().map(|l| l.item).collect(),
        replacements: Vec::new(),
        held_back,
        conflicts,
        picks,
    })
}

/// When the dry run failed: is it because a held package would have to be upgraded too?
pub fn held_block(
    qu: &str,
    print_stdout: &str,
    print_stderr: &str,
) -> Result<Option<HeldBlock>, ParseError> {
    let Some(PrintFailure::Unsatisfiable(unmet)) = parse_print_failure(print_stdout, print_stderr)
    else {
        return Ok(None);
    };
    let held: Vec<String> = parse_qu(qu)?
        .into_iter()
        .filter(|update| update.ignored)
        .map(|update| update.name)
        .collect();
    Ok(unmet.into_iter().find_map(|unmet| {
        let name = unmet.dependency.split(['<', '>', '=']).next()?;
        held.iter().any(|h| h == name).then(|| HeldBlock {
            package: unmet.required_by.clone(),
            held: name.to_string(),
        })
    }))
}

struct Listed<'a> {
    item: PlanItem,
    details: &'a PackageDetails,
}

impl Listed<'_> {
    /// Whether this package is called `name` or provides it.
    fn answers(&self, name: &str) -> bool {
        self.item.package.name == name || self.details.provides.iter().any(|p| p.name == name)
    }
}

/// The names an installed package provides and conflicts with (`-Qi`; the Apps list needs neither).
struct Relations {
    name: String,
    provides: Vec<Dependency>,
    conflicts: Vec<Dependency>,
}

impl Relations {
    fn answers(&self, name: &str) -> bool {
        self.name == name || self.provides.iter().any(|p| p.name == name)
    }
}

fn installed_relations(qi: &str) -> Result<Vec<Relations>, ParseError> {
    split_records(qi)?
        .iter()
        .map(|record| {
            let name = record.get("Name").ok_or(ParseError::MissingField {
                package: None,
                field: "Name",
            })?;
            Ok(Relations {
                name: name.to_string(),
                provides: dependencies(record, name, "Provides")?,
                conflicts: dependencies(record, name, "Conflicts With")?,
            })
        })
        .collect()
}

/// Conflicts between what is going in and what stays, both ways round. Only rules without a version: comparing
/// versions is pacman's job, and a versioned conflict it finds aborts the real run with nothing changed. A package
/// that is being upgraded is judged by its new version, never against itself.
fn conflicts_of(installing: &[&Listed], installed: &[Relations]) -> Vec<Conflict> {
    let staying: Vec<&Relations> = installed
        .iter()
        .filter(|p| !installing.iter().any(|l| l.item.package.name == p.name))
        .collect();
    let mut found: Vec<Conflict> = Vec::new();
    let mut add = |package: &str, installed: &str| {
        let conflict = Conflict {
            package: package.to_string(),
            installed: installed.to_string(),
        };
        if !found.contains(&conflict) {
            found.push(conflict);
        }
    };
    for new in installing {
        let package = &new.item.package.name;
        for old in &staying {
            let new_says = new
                .details
                .conflicts_with
                .iter()
                .any(|c| c.rule.is_none() && old.answers(&c.name));
            let old_says = old
                .conflicts
                .iter()
                .any(|c| c.rule.is_none() && new.answers(&c.name));
            if new_says || old_says {
                add(package, &old.name);
            }
        }
    }
    found
}

fn package_ref(repo: &str, name: &str) -> PackageRef {
    PackageRef {
        repo: repo.to_string(),
        name: name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! saved {
        ($case:literal, $kind:literal) => {
            include_str!(concat!(
                "../../../tests/fixtures/pacman/container/update-",
                $case,
                "-",
                $kind,
                ".txt"
            ))
        };
    }

    macro_rules! plan {
        ($case:literal) => {
            update_plan(
                saved!($case, "qu.stdout"),
                saved!($case, "print.stdout"),
                saved!($case, "si.stdout"),
                saved!($case, "qi.stdout"),
            )
            .unwrap()
        };
    }

    fn names<'a>(items: impl Iterator<Item = &'a PlanItem>) -> Vec<String> {
        items.map(|item| item.package.name.clone()).collect()
    }

    fn dummy(name: &str) -> String {
        format!("tidewise-dummy-{name}")
    }

    #[test]
    fn upgrades_carry_old_and_new_versions_and_a_new_package_is_a_dependency() {
        let plan = plan!("provider");
        assert_eq!(
            names(plan.items.iter()),
            [dummy("upd"), dummy("prov-a"), dummy("upd-virt")]
        );
        let first = &plan.items[0];
        assert_eq!(first.role, Role::Update);
        assert_eq!(first.installed_version.as_deref(), Some("1.0-1"));
        assert_eq!(first.version, "2.0-1");
        assert_eq!(first.download_size, 2505);
        assert!(first.current_size.is_some());
        assert_eq!(plan.items[1].role, Role::Dependency);
        assert_eq!(plan.items[1].installed_version, None);
        assert_eq!(plan.items[1].current_size, None);
        assert!(plan.replacements.is_empty());
        assert!(plan.held_back.is_empty() && plan.conflicts.is_empty());
        assert_eq!(plan.download_total(), 2505 + 2520 + 2533);
    }

    #[test]
    fn a_new_provider_nobody_named_is_a_pick() {
        let plan = plan!("provider");
        assert_eq!(plan.picks.len(), 1);
        assert_eq!(plan.picks[0].dependency, "tidewise-virtual");
        assert_eq!(plan.picks[0].provider.name, dummy("prov-a"));
    }

    #[test]
    fn a_held_package_is_listed_apart() {
        let plan = plan!("held");
        assert_eq!(plan.held_back.len(), 1);
        assert_eq!(plan.held_back[0].name, dummy("upd"));
        assert!(plan.held_back[0].ignored);
        assert_eq!(names(plan.items.iter()), [dummy("upd-virt")]);
    }

    #[test]
    fn a_new_conflict_with_an_installed_package_is_found() {
        let plan = plan!("conflict");
        assert_eq!(
            plan.conflicts,
            [Conflict {
                package: dummy("upd-conf"),
                installed: dummy("conf-x"),
            }]
        );
    }

    /// The conflict the other way round: the installed package names the new one. Written by hand from the
    /// `conflict` fixtures.
    #[test]
    fn an_installed_package_that_conflicts_with_a_listed_one_is_found() {
        let si = saved!("conflict", "si.stdout").replace(
            "Conflicts With  : tidewise-dummy-conf-x",
            "Conflicts With  : None",
        );
        let qi = saved!("conflict", "qi.stdout").replace(
            "Conflicts With  : tidewise-dummy-conf-y",
            "Conflicts With  : tidewise-dummy-upd-conf",
        );
        let plan = update_plan(
            saved!("conflict", "qu.stdout"),
            saved!("conflict", "print.stdout"),
            &si,
            &qi,
        )
        .unwrap();
        assert!(
            plan.conflicts
                .iter()
                .any(|c| c.installed == dummy("conf-x")),
            "{:?}",
            plan.conflicts
        );
    }

    /// A package that provides and conflicts with one name does not conflict with its own old version, and a rule
    /// with a version is left to pacman (comparing versions is not done here).
    #[test]
    fn its_own_old_version_and_versioned_rules_are_not_conflicts() {
        let with = |conflicts: &str| {
            let si = saved!("conflict", "si.stdout").replace(
                "Conflicts With  : tidewise-dummy-conf-x",
                &format!("Conflicts With  : {conflicts}"),
            );
            update_plan(
                saved!("conflict", "qu.stdout"),
                saved!("conflict", "print.stdout"),
                &si,
                saved!("conflict", "qi.stdout"),
            )
            .unwrap()
            .conflicts
        };
        assert!(with("tidewise-dummy-upd-conf").is_empty());
        assert!(with("tidewise-dummy-conf-x<2.0").is_empty());
    }

    #[test]
    fn nothing_listed_and_nothing_held_is_an_empty_plan() {
        let plan = update_plan("", "", "", saved!("conflict", "qi.stdout")).unwrap();
        assert_eq!(plan, UpdatePlan::default());
    }

    #[test]
    fn a_listed_package_without_details_is_an_error() {
        let si = saved!("replace-only", "si.stdout");
        assert!(parse_si(si).unwrap().len() == 1);
        let error = update_plan(
            "",
            saved!("replace", "print.stdout"),
            si,
            saved!("replace", "qi.stdout"),
        );
        assert!(
            matches!(error, Err(ParseError::MissingPackage { .. })),
            "{error:?}"
        );
    }

    #[test]
    fn broken_text_is_an_error_whichever_read_it_is() {
        let good = [
            saved!("provider", "qu.stdout"),
            saved!("provider", "print.stdout"),
            saved!("provider", "si.stdout"),
            saved!("provider", "qi.stdout"),
        ];
        for broken in 0..4 {
            let mut texts = good;
            texts[broken] = "not pacman output\n";
            let result = update_plan(texts[0], texts[1], texts[2], texts[3]);
            assert!(result.is_err(), "read {broken} was accepted");
        }
    }

    #[test]
    fn a_held_package_that_an_upgrade_needs_blocks_the_update() {
        let block = held_block(
            saved!("needs-held", "qu.stdout"),
            saved!("needs-held", "print.stdout"),
            saved!("needs-held", "print.stderr"),
        )
        .unwrap();
        assert_eq!(
            block,
            Some(HeldBlock {
                package: dummy("needs-upd"),
                held: dummy("upd"),
            })
        );
    }

    #[test]
    fn another_failure_is_not_blamed_on_a_held_package() {
        // Nothing held in the -Qu: the same two lines are a plain "nothing satisfies" failure.
        let none = held_block(
            "",
            saved!("needs-held", "print.stdout"),
            saved!("needs-held", "print.stderr"),
        )
        .unwrap();
        assert_eq!(none, None);
        let lock = held_block(
            saved!("needs-held", "qu.stdout"),
            "",
            "error: failed to init transaction (unable to lock database)\n",
        )
        .unwrap();
        assert_eq!(lock, None);
    }
}
