//! Read-only apt / dpkg adapter. Parses `dpkg-query -W -f='${Package}\t${Installed-Size}\t${Status}\n'`
//! and the dry run `apt-get -s remove -- NAME`, both run with `LC_ALL=C`.

use std::path::Path;
use std::process::Command;

use crate::source::{
    is_plain_name, AppEntry, AppSource, Blocked, PlanItem, RemovalPlan, Scope, SourceError,
    SourceKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AptPackage {
    pub name: String,
    /// `None` for packages without a size (meta packages).
    pub installed_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("line {line}: not `package<TAB>size<TAB>status`")]
    BadLine { line: usize },
}

/// Installed packages from `dpkg-query -W`. Packages that were removed but kept their settings
/// (`deinstall ok config-files`) are not installed and are left out.
pub fn parse_dpkg_query(text: &str) -> Result<Vec<AptPackage>, ParseError> {
    let _ = text;
    Ok(Vec::new())
}

/// What a dry-run `apt-get -s remove` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimOutcome {
    Removes {
        /// Everything that would go, in the order apt lists it (the asked-for package and the
        /// packages that depend on it).
        packages: Vec<String>,
        /// Essential packages among them; removing these must never be offered.
        essential: Vec<String>,
    },
    NotFound(String),
    NotInstalled(String),
    /// Another program holds the package manager's lock.
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("this is not output of `apt-get -s remove` in the C locale")]
pub struct Unrecognized;

/// Read the dry run. Text that is not C-locale apt output (no `Reading package lists...` line, other
/// languages, garbage) is an error: a translated "essential" warning must not be missed.
pub fn parse_simulated_remove(text: &str, succeeded: bool) -> Result<SimOutcome, Unrecognized> {
    let _ = (text, succeeded);
    Err(Unrecognized)
}

pub fn plan_from_simulation(
    app: &AppEntry,
    outcome: &SimOutcome,
) -> Result<RemovalPlan, SourceError> {
    let _ = outcome;
    Err(SourceError::BadName(app.id.clone()))
}

/// apt as an app source: every installed package, from `dpkg-query`.
pub struct AptApps {
    packages: Result<Vec<AptPackage>, SourceError>,
}

impl AptApps {
    pub fn from_dpkg(text: &str) -> Self {
        let _ = text;
        AptApps {
            packages: Ok(Vec::new()),
        }
    }

    /// Ask the real dpkg-query (read-only). Not installed means no packages, not an error.
    pub fn system() -> Self {
        let _ = (Path::new("/usr/bin/dpkg-query"), Command::new("true"));
        AptApps {
            packages: Ok(Vec::new()),
        }
    }
}

impl AppSource for AptApps {
    fn kind(&self) -> SourceKind {
        SourceKind::Apt
    }

    fn list(&self) -> Result<Vec<AppEntry>, SourceError> {
        let _ = (&self.packages, Scope::System);
        Ok(Vec::new())
    }

    fn removal_plan(&self, app: &AppEntry) -> Result<RemovalPlan, SourceError> {
        let _ = (
            is_plain_name(&app.id),
            PlanItem {
                label: String::new(),
                bytes: None,
            },
            Blocked::Busy,
        );
        Err(SourceError::BadName(app.id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/apt/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn installed_packages_are_read_with_sizes_in_bytes() {
        let pkgs = parse_dpkg_query(&fx("dpkg-query-handwritten.txt")).unwrap();
        let names: Vec<_> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "bash",
                "libc6:amd64",
                "vlc",
                "vlc-bin",
                "held-thing",
                "meta-pkg",
                "linux-image-5.15.0-91-generic"
            ]
        );
        assert_eq!(
            pkgs[0].installed_size_bytes,
            Some(1864 * 1024),
            "Installed-Size is in KiB"
        );
        let meta = pkgs.iter().find(|p| p.name == "meta-pkg").unwrap();
        assert_eq!(meta.installed_size_bytes, None);
    }

    #[test]
    fn removed_with_settings_left_is_not_installed() {
        let pkgs = parse_dpkg_query(&fx("dpkg-query-handwritten.txt")).unwrap();
        assert!(pkgs.iter().all(|p| p.name != "oldpkg"));
    }

    #[test]
    fn broken_lines_and_sizes_are_errors_with_the_line_number() {
        assert_eq!(
            parse_dpkg_query(&fx("dpkg-query-broken-handwritten.txt")),
            Err(ParseError::BadLine { line: 1 })
        );
        assert_eq!(
            parse_dpkg_query(&fx("dpkg-query-badsize-handwritten.txt")),
            Err(ParseError::BadLine { line: 2 })
        );
        assert_eq!(
            parse_dpkg_query("a\tb\tc\td\n"),
            Err(ParseError::BadLine { line: 1 })
        );
    }

    #[test]
    fn empty_output_is_no_packages() {
        assert_eq!(parse_dpkg_query(""), Ok(vec![]));
        assert_eq!(parse_dpkg_query("\n\n"), Ok(vec![]));
    }

    #[test]
    fn apt_packages_become_system_entries() {
        let apps = AptApps::from_dpkg(&fx("dpkg-query-handwritten.txt"))
            .list()
            .unwrap();
        let vlc = apps.iter().find(|a| a.id == "vlc").unwrap();
        assert_eq!((vlc.kind, vlc.scope), (SourceKind::Apt, Scope::System));
        assert_eq!(vlc.size_bytes, Some(4325 * 1024));
    }

    #[test]
    fn a_broken_dpkg_listing_is_a_source_error() {
        assert!(matches!(
            AptApps::from_dpkg(&fx("dpkg-query-broken-handwritten.txt")).list(),
            Err(SourceError::Parse(_))
        ));
    }

    #[test]
    fn a_dry_run_lists_what_goes_with_the_package() {
        assert_eq!(
            parse_simulated_remove(&fx("apt-sim-remove-ok-handwritten.txt"), true),
            Ok(SimOutcome::Removes {
                packages: vec!["vlc".into(), "vlc-bin".into()],
                essential: vec![]
            })
        );
    }

    #[test]
    fn essential_packages_in_a_dry_run_are_flagged() {
        assert_eq!(
            parse_simulated_remove(&fx("apt-sim-remove-essential-handwritten.txt"), true),
            Ok(SimOutcome::Removes {
                packages: vec!["bash".into(), "coreutils".into()],
                essential: vec!["bash".into(), "coreutils".into()],
            })
        );
    }

    #[test]
    fn unknown_not_installed_and_locked_are_told_apart() {
        assert_eq!(
            parse_simulated_remove(&fx("apt-sim-remove-notfound-handwritten.txt"), false),
            Ok(SimOutcome::NotFound("nosuchpkg".into()))
        );
        assert_eq!(
            parse_simulated_remove(&fx("apt-sim-remove-notinstalled-handwritten.txt"), true),
            Ok(SimOutcome::NotInstalled("oldpkg".into()))
        );
        assert_eq!(
            parse_simulated_remove(&fx("apt-sim-remove-busy-handwritten.txt"), false),
            Ok(SimOutcome::Busy)
        );
    }

    #[test]
    fn other_languages_and_garbage_are_refused_not_guessed() {
        for name in [
            "apt-sim-remove-de-handwritten.txt",
            "apt-sim-remove-garbage-handwritten.txt",
        ] {
            for ok in [true, false] {
                assert_eq!(
                    parse_simulated_remove(&fx(name), ok),
                    Err(Unrecognized),
                    "{name} {ok}"
                );
            }
        }
        for text in ["", "\n", "Remv vlc [1]\n", "Reading package lists...\n"] {
            assert_eq!(
                parse_simulated_remove(text, true),
                Err(Unrecognized),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_hostile_package_name_in_a_remv_line_is_refused() {
        let text = "Reading package lists...\nRemv --force-yes [1]\n";
        assert_eq!(parse_simulated_remove(text, true), Err(Unrecognized));
    }

    fn vlc_app() -> AppEntry {
        AptApps::from_dpkg(&fx("dpkg-query-handwritten.txt"))
            .list()
            .unwrap()
            .into_iter()
            .find(|a| a.id == "vlc")
            .unwrap()
    }

    #[test]
    fn a_normal_dry_run_becomes_a_plan_that_needs_admin() {
        let outcome =
            parse_simulated_remove(&fx("apt-sim-remove-ok-handwritten.txt"), true).unwrap();
        let plan = plan_from_simulation(&vlc_app(), &outcome).unwrap();
        let labels: Vec<_> = plan.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["vlc", "vlc-bin"]);
        assert_eq!(plan.items[0].bytes, Some(4325 * 1024));
        assert!(plan.needs_admin && plan.blocked.is_none() && !plan.to_trash);
        assert_eq!(plan.command, ["apt-get", "remove", "--", "vlc"]);
    }

    #[test]
    fn essential_and_busy_make_blocked_plans() {
        let essential =
            parse_simulated_remove(&fx("apt-sim-remove-essential-handwritten.txt"), true).unwrap();
        let mut bash = vlc_app();
        bash.id = "bash".into();
        let plan = plan_from_simulation(&bash, &essential).unwrap();
        assert_eq!(
            plan.blocked,
            Some(Blocked::Protected(vec!["bash".into(), "coreutils".into()]))
        );
        let plan = plan_from_simulation(&vlc_app(), &SimOutcome::Busy).unwrap();
        assert_eq!(plan.blocked, Some(Blocked::Busy));
    }

    #[test]
    fn plans_for_the_wrong_package_or_bad_names_are_refused() {
        let other = SimOutcome::Removes {
            packages: vec!["bash".into()],
            essential: vec![],
        };
        assert!(plan_from_simulation(&vlc_app(), &other).is_err());
        assert!(plan_from_simulation(&vlc_app(), &SimOutcome::NotFound("vlc".into())).is_err());
        assert!(plan_from_simulation(&vlc_app(), &SimOutcome::NotInstalled("vlc".into())).is_err());
        let mut app = vlc_app();
        for bad in ["--force-yes", "a b", ""] {
            app.id = bad.into();
            assert!(
                matches!(
                    plan_from_simulation(&app, &SimOutcome::Busy),
                    Err(SourceError::BadName(_))
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_estimate_plan_for_the_trait_needs_admin_and_refuses_hostile_ids() {
        let src = AptApps::from_dpkg(&fx("dpkg-query-handwritten.txt"));
        let plan = src.removal_plan(&vlc_app()).unwrap();
        assert!(plan.needs_admin);
        assert_eq!(plan.command, ["apt-get", "remove", "--", "vlc"]);
        let mut app = vlc_app();
        app.id = "--purge".into();
        assert!(matches!(
            src.removal_plan(&app),
            Err(SourceError::BadName(_))
        ));
    }
}
