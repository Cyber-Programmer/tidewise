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
    let mut packages = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let bad = ParseError::BadLine { line: index + 1 };
        let columns: Vec<&str> = line.split('\t').collect();
        let [name, size, status] = columns[..] else {
            return Err(bad);
        };
        let name = name.trim();
        let size = size.trim();
        let status: Vec<&str> = status.split_whitespace().collect();
        if !is_plain_name(name) || status.len() != 3 {
            return Err(bad);
        }
        let installed_size_bytes = if size.is_empty() {
            None
        } else if size.bytes().all(|b| b.is_ascii_digit()) {
            Some(
                size.parse::<u64>()
                    .ok()
                    .and_then(|kib| kib.checked_mul(1024))
                    .ok_or(bad)?,
            )
        } else {
            return Err(bad);
        };
        if status[2] == "installed" {
            packages.push(AptPackage {
                name: name.to_string(),
                installed_size_bytes,
            });
        }
    }
    Ok(packages)
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
    let lines: Vec<&str> = text.lines().collect();
    let first_with = |prefix: &str| lines.iter().find_map(|l| l.trim().strip_prefix(prefix));
    if !succeeded && first_with("E: Could not get lock").is_some() {
        return Ok(SimOutcome::Busy);
    }
    if !lines.iter().any(|l| l.trim() == "Reading package lists...") {
        return Err(Unrecognized);
    }
    if !succeeded {
        return first_with("E: Unable to locate package ")
            .map(str::trim)
            .filter(|n| is_plain_name(n))
            .map(|n| SimOutcome::NotFound(n.to_string()))
            .ok_or(Unrecognized);
    }
    let mut packages = Vec::new();
    let mut essential = Vec::new();
    let mut in_warning = false;
    for line in &lines {
        if line.starts_with("WARNING: The following essential packages will be removed.") {
            in_warning = true;
        } else if in_warning && line.starts_with(char::is_whitespace) {
            let name = line.split_whitespace().next().unwrap_or_default();
            if !is_plain_name(name) {
                return Err(Unrecognized);
            }
            essential.push(name.to_string());
        } else if !line.starts_with("This should NOT be done") {
            in_warning = false;
        }
        if let Some(rest) = line.strip_prefix("Remv ") {
            let name = rest.split_whitespace().next().unwrap_or_default();
            if !is_plain_name(name) {
                return Err(Unrecognized);
            }
            packages.push(name.to_string());
        }
    }
    if !packages.is_empty() {
        return Ok(SimOutcome::Removes {
            packages,
            essential,
        });
    }
    lines
        .iter()
        .find_map(|l| {
            l.strip_prefix("Package '")
                .and_then(|r| r.strip_suffix("' is not installed, so not removed"))
        })
        .filter(|n| is_plain_name(n))
        .map(|n| SimOutcome::NotInstalled(n.to_string()))
        .ok_or(Unrecognized)
}

pub fn plan_from_simulation(
    app: &AppEntry,
    outcome: &SimOutcome,
) -> Result<RemovalPlan, SourceError> {
    if !is_plain_name(&app.id) {
        return Err(SourceError::BadName(app.id.clone()));
    }
    let wrong = || SourceError::Parse("apt-get -s remove".into());
    let (items, blocked) = match outcome {
        SimOutcome::Removes {
            packages,
            essential,
        } => {
            if packages.first().map(String::as_str) != Some(app.id.as_str()) {
                return Err(wrong());
            }
            let items = packages
                .iter()
                .enumerate()
                .map(|(i, name)| PlanItem {
                    label: name.clone(),
                    bytes: if i == 0 { app.size_bytes } else { None },
                })
                .collect();
            (
                items,
                (!essential.is_empty()).then(|| Blocked::Protected(essential.clone())),
            )
        }
        SimOutcome::Busy => (
            vec![PlanItem {
                label: app.id.clone(),
                bytes: app.size_bytes,
            }],
            Some(Blocked::Busy),
        ),
        SimOutcome::NotFound(_) | SimOutcome::NotInstalled(_) => return Err(wrong()),
    };
    Ok(plan(app, items, blocked))
}

fn plan(app: &AppEntry, items: Vec<PlanItem>, blocked: Option<Blocked>) -> RemovalPlan {
    RemovalPlan {
        id: app.id.clone(),
        kind: SourceKind::Apt,
        items,
        blocked,
        needs_admin: true,
        to_trash: false,
        command: ["apt-get", "remove", "--", app.id.as_str()]
            .map(String::from)
            .to_vec(),
    }
}

/// apt as an app source: every installed package, from `dpkg-query`.
pub struct AptApps {
    packages: Result<Vec<AptPackage>, SourceError>,
}

impl AptApps {
    pub fn from_dpkg(text: &str) -> Self {
        AptApps {
            packages: parse_dpkg_query(text).map_err(|_| SourceError::Parse("dpkg-query".into())),
        }
    }

    /// Ask the real dpkg-query (read-only). Not installed means no packages, not an error.
    pub fn system() -> Self {
        let program = "/usr/bin/dpkg-query";
        if !Path::new(program).exists() {
            return AptApps {
                packages: Ok(Vec::new()),
            };
        }
        let out = Command::new(program)
            .args(["-W", r"-f=${Package}\t${Installed-Size}\t${Status}\n"])
            .env("LC_ALL", "C")
            .output();
        match out {
            Ok(o) if o.status.success() => Self::from_dpkg(&String::from_utf8_lossy(&o.stdout)),
            _ => AptApps {
                packages: Err(SourceError::Run("dpkg-query".into())),
            },
        }
    }
}

impl AppSource for AptApps {
    fn kind(&self) -> SourceKind {
        SourceKind::Apt
    }

    fn list(&self) -> Result<Vec<AppEntry>, SourceError> {
        Ok(self
            .packages
            .clone()?
            .into_iter()
            .map(|p| AppEntry {
                id: p.name.clone(),
                name: p.name,
                kind: SourceKind::Apt,
                scope: Scope::System,
                version: None,
                size_bytes: p.installed_size_bytes,
                required_by: Vec::new(),
            })
            .collect())
    }

    /// Cautious estimate: the package itself. The packages that would go with it come from the
    /// dry run (`plan_from_simulation`).
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
