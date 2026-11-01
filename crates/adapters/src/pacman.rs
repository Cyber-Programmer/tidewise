//! Read-only pacman adapter. Parses the machine-readable output of `pacman -Qi`, `-Qq`
//! and `-Qdtq`, always run with `LC_ALL=C`.

use std::path::PathBuf;
use std::process::Command;

use crate::Package;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    /// The text does not look like `pacman -Qi` output in the C locale
    /// (for example it was printed in another language).
    #[error("missing field `{0}` in pacman -Qi output")]
    MissingField(&'static str),
    #[error("cannot read size `{0}`")]
    BadSize(String),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not run pacman: {0}")]
    Run(#[from] std::io::Error),
    #[error("pacman exited with {status}: {stderr}")]
    Failed { status: String, stderr: String },
    #[error(transparent)]
    Parse(#[from] ParseError),
}

/// Runs the system pacman, read-only. No shell; the program is started by absolute path.
pub struct Pacman {
    program: PathBuf,
}

impl Pacman {
    pub fn system() -> Self {
        Self {
            program: PathBuf::from("/usr/bin/pacman"),
        }
    }

    fn run(&self, args: &[&str]) -> Result<std::process::Output, Error> {
        Ok(Command::new(&self.program)
            .args(args)
            .env("LC_ALL", "C")
            .output()?)
    }

    /// Every installed package (`pacman -Qi`).
    pub fn installed(&self) -> Result<Vec<Package>, Error> {
        let out = self.run(&["-Qi"])?;
        if !out.status.success() {
            return Err(failed(&out));
        }
        Ok(parse_qi(&String::from_utf8_lossy(&out.stdout))?)
    }

    /// Packages installed as dependencies that nothing needs any more (`pacman -Qdtq`).
    pub fn orphans(&self) -> Result<Vec<String>, Error> {
        let out = self.run(&["-Qdtq"])?;
        // pacman exits with 1 and prints nothing when there are no orphans.
        if !out.status.success() && !out.stdout.is_empty() {
            return Err(failed(&out));
        }
        Ok(parse_names(&String::from_utf8_lossy(&out.stdout)))
    }
}

fn failed(out: &std::process::Output) -> Error {
    Error::Failed {
        status: out.status.to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
    }
}

/// One name per line, as printed by `pacman -Qq` and `pacman -Qdtq`.
pub fn parse_names(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// Parse the output of `pacman -Qi` for one or many packages.
pub fn parse_qi(text: &str) -> Result<Vec<Package>, ParseError> {
    let mut packages = Vec::new();
    let mut block: Vec<(&str, String)> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            if !block.is_empty() {
                packages.push(package_from_block(&block)?);
                block.clear();
            }
        } else if line.starts_with(char::is_whitespace) {
            // Continuation of a multi-line value such as "Optional Deps".
            if let Some((_, value)) = block.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
        } else if let Some((key, value)) = line.split_once(" : ") {
            block.push((key.trim(), value.trim().to_string()));
        } else if let Some(key) = line.trim_end().strip_suffix(" :") {
            block.push((key.trim(), String::new()));
        }
    }
    if !block.is_empty() {
        packages.push(package_from_block(&block)?);
    }
    Ok(packages)
}

fn package_from_block(block: &[(&str, String)]) -> Result<Package, ParseError> {
    let field = |name: &'static str| {
        block
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
            .ok_or(ParseError::MissingField(name))
    };
    let name = field("Name")?;
    let version = field("Version")?;
    let size = parse_size(field("Installed Size")?)?;
    let required_by = match field("Required By")? {
        "None" => Vec::new(),
        list => list.split_whitespace().map(String::from).collect(),
    };
    Ok(Package {
        name: name.to_string(),
        version: version.to_string(),
        installed_size_bytes: size,
        required_by,
    })
}

/// Turn "9.59 MiB" into bytes.
fn parse_size(text: &str) -> Result<u64, ParseError> {
    let bad = || ParseError::BadSize(text.to_string());
    let (number, unit) = text.split_once(' ').ok_or_else(bad)?;
    let factor: f64 = match unit {
        "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return Err(bad()),
    };
    let number: f64 = number.parse().map_err(|_| bad())?;
    if !number.is_finite() || number < 0.0 {
        return Err(bad());
    }
    Ok((number * factor).round() as u64)
}

use crate::source::{
    is_plain_name, AppEntry, AppSource, Blocked, PlanItem, RemovalPlan, Scope, SourceError,
    SourceKind,
};

/// pacman as an app source: every installed package, from `pacman -Qi`.
pub struct PacmanApps {
    packages: Result<Vec<Package>, SourceError>,
}

impl PacmanApps {
    /// From saved `pacman -Qi` output (tests, fixtures).
    pub fn from_qi(text: &str) -> Self {
        PacmanApps {
            packages: parse_qi(text).map_err(|_| SourceError::Parse("pacman -Qi".into())),
        }
    }

    /// Ask the real pacman (read-only).
    pub fn system() -> Self {
        PacmanApps {
            packages: Pacman::system().installed().map_err(|e| match e {
                Error::Parse(_) => SourceError::Parse("pacman -Qi".into()),
                _ => SourceError::Run("pacman".into()),
            }),
        }
    }
}

impl AppSource for PacmanApps {
    fn kind(&self) -> SourceKind {
        SourceKind::Pacman
    }

    fn list(&self) -> Result<Vec<AppEntry>, SourceError> {
        let packages = self.packages.clone()?;
        Ok(packages
            .into_iter()
            .map(|p| AppEntry {
                id: p.name.clone(),
                name: p.name,
                kind: SourceKind::Pacman,
                scope: Scope::System,
                version: Some(p.version),
                size_bytes: Some(p.installed_size_bytes),
                required_by: p.required_by,
            })
            .collect())
    }

    /// Cautious estimate from the package's own record: the package itself, blocked when others
    /// need it. The exact list `pacman -Rns --print` would give (dependencies that go with it)
    /// comes with the removal-plan parser, not here.
    fn removal_plan(&self, app: &AppEntry) -> Result<RemovalPlan, SourceError> {
        if !is_plain_name(&app.id) {
            return Err(SourceError::BadName(app.id.clone()));
        }
        Ok(RemovalPlan {
            id: app.id.clone(),
            kind: SourceKind::Pacman,
            items: vec![PlanItem {
                label: app.id.clone(),
                bytes: app.size_bytes,
            }],
            blocked: (!app.required_by.is_empty())
                .then(|| Blocked::RequiredBy(app.required_by.clone())),
            needs_admin: true,
            to_trash: false,
            command: ["pacman", "-Rns", "--", app.id.as_str()]
                .map(String::from)
                .to_vec(),
        })
    }
}

/// One package pacman would remove, from `pacman -Rns --print`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub version: String,
}

/// What a dry-run `pacman -Rns --print <name>` said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintOutcome {
    /// These would be removed, the asked-for package and the dependencies that go with it.
    Targets(Vec<Target>),
    /// Refused: these packages still need it.
    Blocked(Vec<String>),
    NotFound(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("this is not output of `pacman -Rns --print` in the C locale")]
pub struct Unrecognized;

/// Read the output of `pacman -Rns --print` (stdout when it succeeded, stderr when it did not),
/// run with `LC_ALL=C`. Anything it does not recognise is an error: a wrong guess could hide a
/// package that something needs.
pub fn parse_removal_print(text: &str, succeeded: bool) -> Result<PrintOutcome, Unrecognized> {
    let _ = (text, succeeded);
    Err(Unrecognized)
}

/// The removal plan for `app` from a dry run's outcome.
pub fn plan_from_print(app: &AppEntry, outcome: &PrintOutcome) -> Result<RemovalPlan, SourceError> {
    let _ = (app, outcome);
    Err(SourceError::BadName(app.id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/pacman/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    fn one(name: &str) -> Package {
        let mut all = parse_qi(&fixture(name)).unwrap();
        assert_eq!(all.len(), 1);
        all.remove(0)
    }

    #[test]
    fn bash_has_many_required_by() {
        let p = one("Qi-bash.txt");
        assert_eq!(p.name, "bash");
        assert_eq!(p.version, "5.3.20-1");
        assert_eq!(
            p.installed_size_bytes,
            (9.59 * 1024.0 * 1024.0_f64).round() as u64
        );
        assert!(p.required_by.len() > 100);
        assert!(p.required_by.contains(&"systemd".to_string()));
        assert!(!p
            .required_by
            .iter()
            .any(|n| n.is_empty() || n.contains(' ')));
    }

    #[test]
    fn package_with_long_optional_deps_still_parses() {
        let p = one("Qi-vlc.txt");
        assert_eq!(p.name, "vlc");
        assert!(p.installed_size_bytes > 0);
        // The optional-deps continuation lines must not leak into other fields.
        assert!(!p.required_by.iter().any(|n| n.contains(':')));
    }

    #[test]
    fn package_nothing_requires_has_empty_required_by() {
        let p = one("Qi-asar.txt");
        assert_eq!(p.name, "asar");
        assert_eq!(
            p.installed_size_bytes,
            (6.30 * 1024.0 * 1024.0_f64).round() as u64
        );
        assert_eq!(p.required_by, Vec::<String>::new());
    }

    #[test]
    fn several_packages_separated_by_blank_lines() {
        let text = format!("{}\n{}", fixture("Qi-asar.txt"), fixture("Qi-bash.txt"));
        let names: Vec<_> = parse_qi(&text)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["asar", "bash"]);
    }

    #[test]
    fn german_output_is_refused_not_misread() {
        assert_eq!(
            parse_qi(&fixture("Qi-asar-de.txt")),
            Err(ParseError::MissingField("Installed Size"))
        );
    }

    #[test]
    fn empty_output_is_no_packages() {
        assert_eq!(parse_qi(&fixture("empty.txt")), Ok(vec![]));
        assert_eq!(parse_names(&fixture("empty.txt")), Vec::<String>::new());
    }

    #[test]
    fn unknown_size_unit_is_an_error() {
        let text = "Name : x\nVersion : 1\nInstalled Size : 3 parsecs\nRequired By : None\n";
        assert_eq!(parse_qi(text), Err(ParseError::BadSize("3 parsecs".into())));
    }

    #[test]
    fn size_units() {
        let size = |s: &str| {
            parse_qi(&format!(
                "Name : x\nVersion : 1\nInstalled Size : {s}\nRequired By : None\n"
            ))
            .unwrap()[0]
                .installed_size_bytes
        };
        assert_eq!(size("512.00 B"), 512);
        assert_eq!(size("1.00 KiB"), 1024);
        assert_eq!(size("1.50 MiB"), 1_572_864);
        assert_eq!(size("2.00 GiB"), 2 * 1024 * 1024 * 1024);
    }

    #[test]
    fn names_one_per_line() {
        let names = parse_names(&fixture("Qq-first40.txt"));
        assert_eq!(names.len(), 40);
        assert!(names
            .iter()
            .all(|n| !n.is_empty() && !n.contains(char::is_whitespace)));
        assert_eq!(parse_names("a\n\nb \n"), ["a", "b"]);
        assert_eq!(parse_names(&fixture("Qdtq-first8.txt")).len(), 8);
    }

    fn qi(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/pacman/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn pacman_packages_become_app_entries() {
        let text = format!("{}\n{}", qi("Qi-vlc.txt"), qi("Qi-bash.txt"));
        let apps = PacmanApps::from_qi(&text).list().unwrap();
        let vlc = apps.iter().find(|a| a.id == "vlc").unwrap();
        assert_eq!(vlc.kind, SourceKind::Pacman);
        assert_eq!(vlc.scope, crate::source::Scope::System);
        assert_eq!(vlc.version.as_deref(), Some("3.0.24-1"));
        assert!(vlc.size_bytes.unwrap() > 40_000_000);
        assert!(vlc.required_by.is_empty());
        assert!(!apps
            .iter()
            .find(|a| a.id == "bash")
            .unwrap()
            .required_by
            .is_empty());
    }

    #[test]
    fn a_package_nothing_needs_has_a_plan_that_needs_admin() {
        let src = PacmanApps::from_qi(&qi("Qi-vlc.txt"));
        let app = src.list().unwrap().remove(0);
        let plan = src.removal_plan(&app).unwrap();
        assert_eq!(plan.blocked, None);
        assert!(plan.needs_admin && !plan.to_trash);
        assert_eq!(plan.items[0].label, "vlc");
        assert_eq!(plan.command, ["pacman", "-Rns", "--", "vlc"]);
    }

    #[test]
    fn a_package_others_need_is_blocked_and_names_them() {
        let src = PacmanApps::from_qi(&qi("Qi-bash.txt"));
        let app = src.list().unwrap().remove(0);
        let plan = src.removal_plan(&app).unwrap();
        match plan.blocked {
            Some(crate::source::Blocked::RequiredBy(who)) => {
                assert!(who.contains(&"alsa-utils".to_string()))
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn output_that_is_not_qi_is_a_source_error_not_an_empty_list() {
        let src = PacmanApps::from_qi("Name : x\n");
        assert!(matches!(src.list(), Err(SourceError::Parse(_))));
    }

    #[test]
    fn a_hostile_id_never_reaches_a_plan() {
        let src = PacmanApps::from_qi(&qi("Qi-vlc.txt"));
        let mut app = src.list().unwrap().remove(0);
        for bad in ["--force", "a b", "../x", ""] {
            app.id = bad.into();
            assert!(
                matches!(src.removal_plan(&app), Err(SourceError::BadName(_))),
                "{bad:?}"
            );
        }
    }

    fn rm(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/pacman/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    fn target(name: &str, version: &str) -> Target {
        Target {
            name: name.into(),
            version: version.into(),
        }
    }

    #[test]
    fn targets_are_split_at_the_last_two_dashes_so_dashed_names_survive() {
        assert_eq!(
            parse_removal_print(&rm("Rns-print-ok-handwritten.txt"), true),
            Ok(PrintOutcome::Targets(vec![
                target("vlc", "3.0.24-1"),
                target("libdvdcss", "1.4.3-3"),
                target("lib32-glibc", "2.40-2"),
                target("foo-bar-baz", "1:2.0.1-10"),
            ]))
        );
    }

    #[test]
    fn a_refused_removal_names_each_package_that_needs_it_once() {
        assert_eq!(
            parse_removal_print(&rm("Rns-print-blocked-handwritten.txt"), false),
            Ok(PrintOutcome::Blocked(vec![
                "alsa-utils".into(),
                "at-spi2-core".into()
            ]))
        );
    }

    #[test]
    fn an_unknown_name_is_not_found() {
        assert_eq!(
            parse_removal_print(&rm("Rns-print-notfound-handwritten.txt"), false),
            Ok(PrintOutcome::NotFound("nosuchpkg".into()))
        );
    }

    #[test]
    fn other_languages_and_garbage_are_refused_not_guessed() {
        assert_eq!(
            parse_removal_print(&rm("Rns-print-blocked-de-handwritten.txt"), false),
            Err(Unrecognized)
        );
        assert_eq!(
            parse_removal_print(&rm("Rns-print-garbage-handwritten.txt"), true),
            Err(Unrecognized)
        );
        for text in [
            "",
            "\n\n",
            "vlc",
            "vlc-1",
            "-1-2",
            "a b-1-2",
            "error: something else\n",
        ] {
            for ok in [true, false] {
                assert_eq!(
                    parse_removal_print(text, ok),
                    Err(Unrecognized),
                    "{text:?} {ok}"
                );
            }
        }
    }

    #[test]
    fn success_text_is_not_read_as_a_failure_and_the_other_way_round() {
        assert_eq!(
            parse_removal_print(&rm("Rns-print-blocked-handwritten.txt"), true),
            Err(Unrecognized)
        );
        assert_eq!(
            parse_removal_print(&rm("Rns-print-ok-handwritten.txt"), false),
            Err(Unrecognized)
        );
    }

    #[test]
    fn a_dry_run_that_lists_dependencies_becomes_a_plan_with_all_of_them() {
        let src = PacmanApps::from_qi(&qi("Qi-vlc.txt"));
        let app = src.list().unwrap().remove(0);
        let outcome = parse_removal_print(&rm("Rns-print-ok-handwritten.txt"), true).unwrap();
        let plan = plan_from_print(&app, &outcome).unwrap();
        let labels: Vec<_> = plan.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["vlc", "libdvdcss", "lib32-glibc", "foo-bar-baz"]);
        assert_eq!(
            plan.items[0].bytes, app.size_bytes,
            "only the asked-for package has a known size"
        );
        assert_eq!(plan.items[1].bytes, None);
        assert!(plan.needs_admin && plan.blocked.is_none());
        assert_eq!(plan.command, ["pacman", "-Rns", "--", "vlc"]);
    }

    #[test]
    fn a_dry_run_that_was_refused_becomes_a_blocked_plan() {
        let src = PacmanApps::from_qi(&qi("Qi-vlc.txt"));
        let app = src.list().unwrap().remove(0);
        let outcome = parse_removal_print(&rm("Rns-print-blocked-handwritten.txt"), false).unwrap();
        let plan = plan_from_print(&app, &outcome).unwrap();
        assert_eq!(
            plan.blocked,
            Some(Blocked::RequiredBy(vec![
                "alsa-utils".into(),
                "at-spi2-core".into()
            ]))
        );
    }

    #[test]
    fn not_found_and_hostile_ids_give_no_plan() {
        let src = PacmanApps::from_qi(&qi("Qi-vlc.txt"));
        let mut app = src.list().unwrap().remove(0);
        assert!(plan_from_print(&app, &PrintOutcome::NotFound("vlc".into())).is_err());
        app.id = "--force".into();
        assert!(matches!(
            plan_from_print(&app, &PrintOutcome::Targets(vec![target("x", "1-1")])),
            Err(SourceError::BadName(_))
        ));
    }

    #[test]
    fn the_dry_run_must_start_with_the_asked_for_package() {
        let src = PacmanApps::from_qi(&qi("Qi-vlc.txt"));
        let app = src.list().unwrap().remove(0);
        let other = PrintOutcome::Targets(vec![target("bash", "5.3-1")]);
        assert!(
            plan_from_print(&app, &other).is_err(),
            "a plan for a different package is no plan"
        );
    }
}
