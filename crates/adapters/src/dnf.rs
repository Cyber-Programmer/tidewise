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
    let mut packages: Vec<RpmPackage> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let bad = ParseError::BadLine { line: index + 1 };
        let columns: Vec<&str> = line.split('\t').collect();
        let [name, size] = columns[..] else {
            return Err(bad);
        };
        let name = name.trim();
        let size: u64 = size.trim().parse().map_err(|_| bad)?;
        if !is_plain_name(name) {
            return Err(bad);
        }
        if name == "gpg-pubkey" {
            continue;
        }
        match packages.iter_mut().find(|p| p.name == name) {
            Some(existing) => existing.size_bytes = existing.size_bytes.saturating_add(size),
            None => packages.push(RpmPackage {
                name: name.to_string(),
                size_bytes: size,
            }),
        }
    }
    Ok(packages)
}

/// What a dry-run `dnf remove --assumeno` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryRun {
    Removes {
        /// The asked-for package first, then the dependencies that go with it. Empty when dnf
        /// refused before listing anything.
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

/// Read the dry run (stdout and stderr together). `--assumeno` makes dnf exit with an error even
/// when the plan is fine, so the exit status says nothing and only the text counts. Anything that is
/// not C-locale dnf 4 output is an error, never a guess.
pub fn parse_dry_run(text: &str) -> Result<DryRun, Unrecognized> {
    let lines: Vec<&str> = text.lines().collect();
    let after = |prefix: &str| lines.iter().find_map(|l| l.trim().strip_prefix(prefix));
    if after("Error: Failed to obtain the transaction lock").is_some()
        || after("Waiting for process with pid").is_some()
    {
        return Ok(DryRun::Busy);
    }
    if let Some(name) = after("No match for argument: ") {
        let name = name.trim();
        return if is_plain_name(name) {
            Ok(DryRun::NotFound(name.to_string()))
        } else {
            Err(Unrecognized)
        };
    }
    if let Some(list) = lines
        .iter()
        .find_map(|l| l.split_once("removing the following protected packages: "))
        .map(|(_, list)| list)
    {
        let protected: Vec<String> = list.split(',').map(|n| n.trim().to_string()).collect();
        return if protected.iter().all(|n| is_plain_name(n)) {
            Ok(DryRun::Removes {
                packages: Vec::new(),
                protected,
            })
        } else {
            Err(Unrecognized)
        };
    }
    if !lines.iter().any(|l| l.trim() == "Dependencies resolved.")
        || !lines.iter().any(|l| l.trim() == "Transaction Summary")
    {
        return Err(Unrecognized);
    }
    let mut packages = Vec::new();
    let mut in_rows = false;
    let mut pending: Option<&str> = None;
    for line in &lines {
        let trimmed = line.trim();
        if trimmed == "Transaction Summary" {
            break;
        }
        if trimmed == "Removing:" || trimmed == "Removing unused dependencies:" {
            in_rows = true;
            continue;
        }
        if !line.starts_with(' ') && trimmed.ends_with(':') && !trimmed.contains(' ') {
            // Installing:, Upgrading:, … : not a removal.
            return Err(Unrecognized);
        }
        if !in_rows || !line.starts_with(' ') || trimmed.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let name = match (tokens.len(), pending.take()) {
            (1, None) => {
                pending = Some(tokens[0]);
                continue;
            }
            (n, Some(name)) if n >= 5 => name,
            (n, None) if n >= 5 => tokens[0],
            _ => return Err(Unrecognized),
        };
        if !is_plain_name(name) {
            return Err(Unrecognized);
        }
        packages.push(name.to_string());
    }
    if packages.is_empty() || pending.is_some() {
        return Err(Unrecognized);
    }
    Ok(DryRun::Removes {
        packages,
        protected: Vec::new(),
    })
}

pub fn plan_from_dry_run(app: &AppEntry, outcome: &DryRun) -> Result<RemovalPlan, SourceError> {
    if !is_plain_name(&app.id) {
        return Err(SourceError::BadName(app.id.clone()));
    }
    let wrong = || SourceError::Parse("dnf remove --assumeno".into());
    let own = || PlanItem {
        label: app.id.clone(),
        bytes: app.size_bytes,
    };
    let (items, blocked) = match outcome {
        DryRun::Removes {
            packages,
            protected,
        } => {
            if packages.is_empty() && protected.is_empty() {
                return Err(wrong());
            }
            if !packages.is_empty() && packages[0] != app.id {
                return Err(wrong());
            }
            let items = if packages.is_empty() {
                vec![own()]
            } else {
                packages
                    .iter()
                    .enumerate()
                    .map(|(i, name)| PlanItem {
                        label: name.clone(),
                        bytes: if i == 0 { app.size_bytes } else { None },
                    })
                    .collect()
            };
            (
                items,
                (!protected.is_empty()).then(|| Blocked::Protected(protected.clone())),
            )
        }
        DryRun::Busy => (vec![own()], Some(Blocked::Busy)),
        DryRun::NotFound(_) => return Err(wrong()),
    };
    Ok(plan(app, items, blocked))
}

fn plan(app: &AppEntry, items: Vec<PlanItem>, blocked: Option<Blocked>) -> RemovalPlan {
    RemovalPlan {
        id: app.id.clone(),
        kind: SourceKind::Dnf,
        items,
        blocked,
        needs_admin: true,
        to_trash: false,
        command: ["dnf", "remove", "--", app.id.as_str()]
            .map(String::from)
            .to_vec(),
    }
}

/// dnf as an app source: every installed package, from `rpm -qa`.
pub struct DnfApps {
    packages: Result<Vec<RpmPackage>, SourceError>,
}

impl DnfApps {
    pub fn from_rpm(text: &str) -> Self {
        DnfApps {
            packages: parse_rpm_qa(text).map_err(|_| SourceError::Parse("rpm -qa".into())),
        }
    }

    /// Ask the real rpm (read-only). Not installed means no packages, not an error.
    pub fn system() -> Self {
        let program = "/usr/bin/rpm";
        if !Path::new(program).exists() {
            return DnfApps {
                packages: Ok(Vec::new()),
            };
        }
        let out = Command::new(program)
            .args(["-qa", "--qf", r"%{NAME}\t%{SIZE}\n"])
            .env("LC_ALL", "C")
            .output();
        match out {
            Ok(o) if o.status.success() => Self::from_rpm(&String::from_utf8_lossy(&o.stdout)),
            _ => DnfApps {
                packages: Err(SourceError::Run("rpm".into())),
            },
        }
    }
}

impl AppSource for DnfApps {
    fn kind(&self) -> SourceKind {
        SourceKind::Dnf
    }

    fn list(&self) -> Result<Vec<AppEntry>, SourceError> {
        Ok(self
            .packages
            .clone()?
            .into_iter()
            .map(|p| AppEntry {
                id: p.name.clone(),
                name: p.name,
                kind: SourceKind::Dnf,
                scope: Scope::System,
                version: None,
                size_bytes: Some(p.size_bytes),
                required_by: Vec::new(),
            })
            .collect())
    }

    /// Cautious estimate: the package itself. The dependencies that would go with it come from
    /// the dry run (`plan_from_dry_run`).
    fn removal_plan(&self, app: &AppEntry) -> Result<RemovalPlan, SourceError> {
        if !is_plain_name(&app.id) {
            return Err(SourceError::BadName(app.id.clone()));
        }
        Ok(plan(
            app,
            vec![PlanItem {
                label: app.id.clone(),
                bytes: app.size_bytes,
            }],
            None,
        ))
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
            parse_dry_run(&fx("dnf-remove-ok-handwritten.txt")),
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
            parse_dry_run(&fx("dnf-remove-protected-handwritten.txt")),
            Ok(DryRun::Removes {
                packages: vec![],
                protected: vec!["systemd".into(), "dnf".into()]
            })
        );
        assert_eq!(
            parse_dry_run(&fx("dnf-remove-notfound-handwritten.txt")),
            Ok(DryRun::NotFound("nosuchpkg".into()))
        );
        assert_eq!(
            parse_dry_run(&fx("dnf-remove-busy-handwritten.txt")),
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
                assert_eq!(parse_dry_run(&fx(name)), Err(Unrecognized), "{name} {ok}");
            }
        }
        for text in [
            "",
            "\n",
            "Removing:\n vlc x86_64 1-1 @x 1 k\n",
            "Dependencies resolved.\n",
        ] {
            assert_eq!(parse_dry_run(text), Err(Unrecognized), "{text:?}");
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
        let outcome = parse_dry_run(&fx("dnf-remove-ok-handwritten.txt")).unwrap();
        let plan = plan_from_dry_run(&vlc_app(), &outcome).unwrap();
        assert_eq!(plan.items.len(), 3);
        assert_eq!(plan.items[0].bytes, Some(4_325_000));
        assert_eq!(plan.items[1].bytes, None);
        assert!(plan.needs_admin && plan.blocked.is_none());
        assert_eq!(plan.command, ["dnf", "remove", "--", "vlc"]);
    }

    #[test]
    fn protected_and_busy_make_blocked_plans() {
        let protected = parse_dry_run(&fx("dnf-remove-protected-handwritten.txt")).unwrap();
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
