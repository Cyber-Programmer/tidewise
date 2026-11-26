//! The review list of "Update system" (WBS 6.5): what `pacman -Su` would do, worked out from four reads of the same
//! package lists: `-Qu`, the dry run (`-Su --print`), `-Si` of what it lists and `-Qi` of everything installed. Pure:
//! text in, a plan out. The facts it rests on are measured in `tests/container/tests/pacman_update.rs` (decision 0009).
//!
//! What the dry run does not say, and this works out:
//! - it lists a replacement, and what only the replacement needs, but the real run (`--ask 66`) installs neither;
//! - it says nothing about a conflict, and the real run then aborts the whole update;
//! - it takes the first provider of a virtual name without saying so (golden rule 10: the helper refuses that).

use crate::{PackageRef, ParseError, PendingUpdate, PlanItem, ProviderPick};

/// A package that would replace an installed one. Tidewise does not do replacements: it is listed but skipped.
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
    /// Listed packages the real run does not install: replacements, and what only they need.
    pub skipped: Vec<PackageRef>,
    pub replacements: Vec<Replacement>,
    /// `[ignored]` updates: held back by `IgnorePkg` / `IgnoreGroup`.
    pub held_back: Vec<PendingUpdate>,
    pub conflicts: Vec<Conflict>,
    /// Names pacman would answer itself with a new package (the helper refuses these).
    pub picks: Vec<ProviderPick>,
}

impl UpdatePlan {
    /// The packages the real run installs or upgrades.
    pub fn installing(&self) -> impl Iterator<Item = &PlanItem> {
        self.items
            .iter()
            .filter(|item| !self.skipped.contains(&item.package))
    }

    /// Bytes still to download for those.
    pub fn download_total(&self) -> u64 {
        self.installing().map(|item| item.download_size).sum()
    }

    /// How much more (or less) disk space those take afterwards, in bytes.
    pub fn size_change(&self) -> i64 {
        self.installing()
            .map(|item| {
                let new = i64::try_from(item.installed_size).unwrap_or(i64::MAX);
                let old = i64::try_from(item.current_size.unwrap_or(0)).unwrap_or(i64::MAX);
                new.saturating_sub(old)
            })
            .fold(0, i64::saturating_add)
    }
}

/// The plan, from `-Qu`, the dry run, `-Si` of the printed packages and `-Qi` of everything installed.
pub fn update_plan(
    _qu: &str,
    _print: &str,
    _si: &str,
    _qi: &str,
) -> Result<UpdatePlan, ParseError> {
    Ok(UpdatePlan::default())
}

/// When the dry run failed: is it because a held package would have to be upgraded too?
pub fn held_block(
    _qu: &str,
    _print_stdout: &str,
    _print_stderr: &str,
) -> Result<Option<HeldBlock>, ParseError> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Role, parse_si};

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
        assert!(plan.skipped.is_empty() && plan.replacements.is_empty());
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
    fn a_replacement_is_listed_but_not_installed() {
        let plan = plan!("replace");
        assert_eq!(plan.items.len(), 2);
        assert_eq!(
            plan.replacements,
            [Replacement {
                new: PackageRef {
                    repo: "tidewise-test".to_string(),
                    name: dummy("new"),
                },
                old: dummy("old"),
            }]
        );
        assert_eq!(names(plan.installing()), [dummy("upd")]);
        assert_eq!(plan.download_total(), 2505);
        // The replacer conflicts with nothing that matters: it is not installed.
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn with_only_a_replacement_nothing_is_installed() {
        let plan = plan!("replace-only");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.replacements.len(), 1);
        assert_eq!(plan.installing().count(), 0);
        assert_eq!(plan.download_total(), 0);
        assert_eq!(plan.size_change(), 0);
    }

    #[test]
    fn what_only_a_replacement_needs_is_skipped_too() {
        let plan = plan!("replace-dep");
        assert_eq!(
            plan.skipped
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>(),
            [dummy("hello"), dummy("new-dep")]
        );
        assert_eq!(plan.installing().count(), 0);
        assert_eq!(plan.replacements.len(), 1);
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
            "Conflicts With  : None\n",
            "Conflicts With  : tidewise-dummy-upd-conf\n",
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
