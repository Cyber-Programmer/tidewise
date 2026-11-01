//! Read-only dnf / rpm adapter. Parses `rpm -qa --qf '%{NAME}\t%{SIZE}\n'` and the dry run
//! `dnf remove --assumeno -- NAME` (dnf 4 table layout), both run with `LC_ALL=C`.

use std::path::Path;
use std::process::Command;

use crate::source::{
    is_plain_name, AppEntry, AppSource, Blocked, PlanItem, RemovalPlan, Scope, SourceError,
    SourceKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpmPackage {
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("line {line}: not `name<TAB>size`")]
    BadLine { line: usize },
}

/// Installed packages from `rpm -qa`. Several versions of one package (kernels) are one entry with
/// the sizes added; `gpg-pubkey` entries are keys, not software, and are left out.
pub fn parse_rpm_qa(text: &str) -> Result<Vec<RpmPackage>, ParseError> {
    let _ = text;
    Ok(Vec::new())
}

/// What a dry-run `dnf remove --assumeno` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryRun {
    Removes {
        /// The asked-for package first, then the dependencies that go with it.
        packages: Vec<String>,
        /// Protected packages (the system needs them); removing these must never be offered.
        protected: Vec<String>,
    },
    NotFound(String),
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("this is not output of `dnf remove --assumeno` in the C locale")]
pub struct Unrecognized;

/// Read the dry run. Anything that is not C-locale dnf 4 output is an error, never a guess.
pub fn parse_dry_run(text: &str, succeeded: bool) -> Result<DryRun, Unrecognized> {
    let _ = (text, succeeded);
    Err(Unrecognized)
}

pub fn plan_from_dry_run(app: &AppEntry, outcome: &DryRun) -> Result<RemovalPlan, SourceError> {
    let _ = outcome;
    Err(SourceError::BadName(app.id.clone()))
}

/// dnf as an app source: every installed package, from `rpm -qa`.
pub struct DnfApps {
    packages: Result<Vec<RpmPackage>, SourceError>,
}

impl DnfApps {
    pub fn from_rpm(text: &str) -> Self {
        let _ = text;
        DnfApps {
            packages: Ok(Vec::new()),
        }
    }

    pub fn system() -> Self {
        let _ = (Path::new("/usr/bin/rpm"), Command::new("true"));
        DnfApps {
            packages: Ok(Vec::new()),
        }
    }
}

impl AppSource for DnfApps {
    fn kind(&self) -> SourceKind {
        SourceKind::Dnf
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
            "{}/../../tests/fixtures/dnf/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn rpm_packages_are_read_with_sizes_and_versions_merged() {
        let pkgs = parse_rpm_qa(&fx("rpm-qa-handwritten.txt")).unwrap();
        let names: Vec<_> = pkgs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "bash",
                "vlc",
                "vlc-libs",
                "kernel-core",
                "fedora-release-common",
                "meta-thing"
            ]
        );
        assert_eq!(pkgs[0].size_bytes, 1_864_567);
        let kernel = pkgs.iter().find(|p| p.name == "kernel-core").unwrap();
        assert_eq!(
            kernel.size_bytes, 141_000_000,
            "two installed kernels are added up"
        );
    }

    #[test]
    fn broken_rpm_lines_are_errors_with_the_line_number() {
        assert_eq!(
            parse_rpm_qa(&fx("rpm-qa-broken-handwritten.txt")),
            Err(ParseError::BadLine { line: 1 })
        );
        assert_eq!(
            parse_rpm_qa(&fx("rpm-qa-badsize-handwritten.txt")),
            Err(ParseError::BadLine { line: 2 })
        );
        assert_eq!(
            parse_rpm_qa("a\t1\tx\n"),
            Err(ParseError::BadLine { line: 1 })
        );
        assert_eq!(
            parse_rpm_qa("--force\t1\n"),
            Err(ParseError::BadLine { line: 1 })
        );
        assert_eq!(
            parse_rpm_qa("a\t99999999999999999999999\n"),
            Err(ParseError::BadLine { line: 1 })
        );
    }

    #[test]
    fn empty_rpm_output_is_no_packages() {
        assert_eq!(parse_rpm_qa(""), Ok(vec![]));
    }

    #[test]
    fn rpm_packages_become_system_entries_and_a_broken_listing_is_a_source_error() {
        let apps = DnfApps::from_rpm(&fx("rpm-qa-handwritten.txt"))
            .list()
            .unwrap();
        let vlc = apps.iter().find(|a| a.id == "vlc").unwrap();
        assert_eq!(
            (vlc.kind, vlc.scope, vlc.size_bytes),
            (SourceKind::Dnf, Scope::System, Some(4_325_000))
        );
        assert!(matches!(
            DnfApps::from_rpm(&fx("rpm-qa-broken-handwritten.txt")).list(),
            Err(SourceError::Parse(_))
        ));
    }

    #[test]
    fn a_dry_run_lists_the_package_and_its_unused_dependencies_including_wrapped_names() {
        assert_eq!(
            parse_dry_run(&fx("dnf-remove-ok-handwritten.txt"), true),
            Ok(DryRun::Removes {
                packages: vec![
                    "vlc".into(),
                    "libdvdcss".into(),
                    "some-very-long-package-name-that-wraps-the-line".into()
                ],
                protected: vec![],
            })
        );
    }

    #[test]
    fn protected_packages_unknown_names_and_locks_are_told_apart() {
        assert_eq!(
            parse_dry_run(&fx("dnf-remove-protected-handwritten.txt"), false),
            Ok(DryRun::Removes {
                packages: vec![],
                protected: vec!["systemd".into(), "dnf".into()]
            })
        );
        assert_eq!(
            parse_dry_run(&fx("dnf-remove-notfound-handwritten.txt"), false),
            Ok(DryRun::NotFound("nosuchpkg".into()))
        );
        assert_eq!(
            parse_dry_run(&fx("dnf-remove-busy-handwritten.txt"), false),
            Ok(DryRun::Busy)
        );
    }

    #[test]
    fn other_languages_garbage_and_empty_text_are_refused_not_guessed() {
        for name in [
            "dnf-remove-de-handwritten.txt",
            "dnf-remove-garbage-handwritten.txt",
        ] {
            for ok in [true, false] {
                assert_eq!(
                    parse_dry_run(&fx(name), ok),
                    Err(Unrecognized),
                    "{name} {ok}"
                );
            }
        }
        for text in [
            "",
            "\n",
            "Removing:\n vlc x86_64 1-1 @x 1 k\n",
            "Dependencies resolved.\n",
        ] {
            for ok in [true, false] {
                assert_eq!(parse_dry_run(text, ok), Err(Unrecognized), "{text:?}");
            }
        }
    }

    fn vlc_app() -> AppEntry {
        DnfApps::from_rpm(&fx("rpm-qa-handwritten.txt"))
            .list()
            .unwrap()
            .into_iter()
            .find(|a| a.id == "vlc")
            .unwrap()
    }

    #[test]
    fn a_dry_run_becomes_a_plan_that_needs_admin() {
        let outcome = parse_dry_run(&fx("dnf-remove-ok-handwritten.txt"), true).unwrap();
        let plan = plan_from_dry_run(&vlc_app(), &outcome).unwrap();
        assert_eq!(plan.items.len(), 3);
        assert_eq!(plan.items[0].bytes, Some(4_325_000));
        assert_eq!(plan.items[1].bytes, None);
        assert!(plan.needs_admin && plan.blocked.is_none());
        assert_eq!(plan.command, ["dnf", "remove", "--", "vlc"]);
    }

    #[test]
    fn protected_and_busy_make_blocked_plans() {
        let protected = parse_dry_run(&fx("dnf-remove-protected-handwritten.txt"), false).unwrap();
        let plan = plan_from_dry_run(&vlc_app(), &protected).unwrap();
        assert_eq!(
            plan.blocked,
            Some(Blocked::Protected(vec!["systemd".into(), "dnf".into()]))
        );
        assert_eq!(
            plan_from_dry_run(&vlc_app(), &DryRun::Busy)
                .unwrap()
                .blocked,
            Some(Blocked::Busy)
        );
    }

    #[test]
    fn plans_for_the_wrong_package_unknown_names_or_bad_ids_are_refused() {
        let other = DryRun::Removes {
            packages: vec!["bash".into()],
            protected: vec![],
        };
        assert!(plan_from_dry_run(&vlc_app(), &other).is_err());
        assert!(plan_from_dry_run(&vlc_app(), &DryRun::NotFound("vlc".into())).is_err());
        let mut app = vlc_app();
        for bad in ["--allowerasing", "a b", ""] {
            app.id = bad.into();
            assert!(
                matches!(
                    plan_from_dry_run(&app, &DryRun::Busy),
                    Err(SourceError::BadName(_))
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn the_estimate_plan_for_the_trait_needs_admin_and_refuses_hostile_ids() {
        let src = DnfApps::from_rpm(&fx("rpm-qa-handwritten.txt"));
        let plan = src.removal_plan(&vlc_app()).unwrap();
        assert!(plan.needs_admin);
        assert_eq!(plan.command, ["dnf", "remove", "--", "vlc"]);
        let mut app = vlc_app();
        app.id = "--allowerasing".into();
        assert!(matches!(
            src.removal_plan(&app),
            Err(SourceError::BadName(_))
        ));
    }
}
