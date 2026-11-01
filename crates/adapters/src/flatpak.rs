//! Read-only Flatpak listing. Parses
//! `flatpak list --app --columns=application,name,size,installation`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatpakApp {
    pub application_id: String,
    pub name: String,
    /// `None` when flatpak printed a size we do not understand.
    pub size_bytes: Option<u64>,
    /// `system` or `user`.
    pub installation: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("line {line}: expected 4 tab-separated columns, found {found}")]
    WrongColumnCount { line: usize, found: usize },
    #[error(
        "line {line}: not a runtime line (`ref<TAB>size<TAB>installation`, ref = id/arch/branch)"
    )]
    BadRuntimeLine { line: usize },
    #[error("line {line}: not an app line (`application<TAB>runtime<TAB>installation`)")]
    BadAppRuntimeLine { line: usize },
}

/// An installed runtime or runtime extension, from
/// `flatpak list --runtime --columns=ref,size,installation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    /// `id/arch/branch`, as flatpak prints it.
    pub reference: String,
    pub size_bytes: Option<u64>,
    /// `system` or `user`.
    pub installation: String,
}

/// The runtime an installed app runs on, from
/// `flatpak list --app --columns=application,runtime,installation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRuntime {
    pub application_id: String,
    /// `id/arch/branch`.
    pub runtime: String,
    pub installation: String,
}

/// Runtimes split by what we can say about them without asking flatpak.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnusedRuntimes {
    /// Base runtimes (`….Platform`, `….Sdk`) that no installed app uses.
    pub unused: Vec<Runtime>,
    /// Not listed as unused although no app names them, with the reason.
    pub kept: Vec<(Runtime, &'static str)>,
}

pub fn parse_list(text: &str) -> Result<Vec<FlatpakApp>, ParseError> {
    let mut apps = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        let [application_id, name, size, installation] = columns[..] else {
            return Err(ParseError::WrongColumnCount {
                line: index + 1,
                found: columns.len(),
            });
        };
        apps.push(FlatpakApp {
            application_id: application_id.trim().to_string(),
            name: name.trim().to_string(),
            size_bytes: parse_size(size),
            installation: installation.trim().to_string(),
        });
    }
    Ok(apps)
}

pub fn parse_runtimes(text: &str) -> Result<Vec<Runtime>, ParseError> {
    let mut runtimes = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let line_no = index + 1;
        let columns: Vec<&str> = line.split('\t').collect();
        let [reference, size, installation] = columns[..] else {
            return Err(ParseError::BadRuntimeLine { line: line_no });
        };
        let reference = reference.trim();
        if reference.split('/').filter(|p| !p.is_empty()).count() != 3 {
            return Err(ParseError::BadRuntimeLine { line: line_no });
        }
        runtimes.push(Runtime {
            reference: reference.to_string(),
            size_bytes: parse_size(size),
            installation: installation.trim().to_string(),
        });
    }
    Ok(runtimes)
}

pub fn parse_app_runtimes(text: &str) -> Result<Vec<AppRuntime>, ParseError> {
    let mut apps = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        let [application_id, runtime, installation] = columns[..] else {
            return Err(ParseError::BadAppRuntimeLine { line: index + 1 });
        };
        apps.push(AppRuntime {
            application_id: application_id.trim().to_string(),
            runtime: runtime.trim().to_string(),
            installation: installation.trim().to_string(),
        });
    }
    Ok(apps)
}

/// Which runtimes no installed app uses. Deliberately cautious, because this list only uses what
/// `flatpak list` prints and `flatpak uninstall --unused` knows more (it also follows runtimes built
/// on other runtimes and extensions): only base runtimes (`….Platform`, `….Sdk`) are ever offered,
/// extensions (GL, codecs, locale, themes) and the freedesktop base (other runtimes sit on it) are
/// kept. A runtime counts as used when an app in *any* installation names its exact reference.
pub fn unused_runtimes(runtimes: &[Runtime], apps: &[AppRuntime]) -> UnusedRuntimes {
    let used: std::collections::HashSet<&str> = apps.iter().map(|a| a.runtime.as_str()).collect();
    let mut result = UnusedRuntimes::default();
    for runtime in runtimes {
        if used.contains(runtime.reference.as_str()) {
            continue;
        }
        let id = runtime.reference.split('/').next().unwrap_or_default();
        if !(id.ends_with(".Platform") || id.ends_with(".Sdk")) {
            result
                .kept
                .push((runtime.clone(), "an extension of another runtime"));
        } else if id.starts_with("org.freedesktop.") {
            result
                .kept
                .push((runtime.clone(), "other runtimes may be built on it"));
        } else {
            result.unused.push(runtime.clone());
        }
    }
    result
}

/// Flatpak prints sizes like `18.1 MB` (decimal units, no-break space, locale decimal separator).
fn parse_size(text: &str) -> Option<u64> {
    let (number, unit) = text.trim().split_once(char::is_whitespace)?;
    let factor: f64 = match unit.trim() {
        "bytes" | "byte" | "B" => 1.0,
        "kB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    let number: f64 = number.replace(',', ".").parse().ok()?;
    (number.is_finite() && number >= 0.0).then(|| (number * factor).round() as u64)
}

use crate::source::{
    is_plain_name, AppEntry, AppSource, PlanItem, RemovalPlan, Scope, SourceError, SourceKind,
};

/// Flatpak apps (user and system installations) as an app source.
pub struct FlatpakApps {
    apps: Result<Vec<FlatpakApp>, SourceError>,
}

impl FlatpakApps {
    /// From saved `flatpak list --app --columns=application,name,size,installation` output.
    pub fn from_list(text: &str) -> Self {
        FlatpakApps {
            apps: parse_list(text).map_err(|_| SourceError::Parse("flatpak list".into())),
        }
    }

    /// Ask the real flatpak (read-only). Not installed means no apps, not an error.
    pub fn system() -> Self {
        let program = "/usr/bin/flatpak";
        if !std::path::Path::new(program).exists() {
            return FlatpakApps {
                apps: Ok(Vec::new()),
            };
        }
        let out = std::process::Command::new(program)
            .args([
                "list",
                "--app",
                "--columns=application,name,size,installation",
            ])
            .env("LC_ALL", "C")
            .output();
        match out {
            Ok(o) if o.status.success() => Self::from_list(&String::from_utf8_lossy(&o.stdout)),
            _ => FlatpakApps {
                apps: Err(SourceError::Run("flatpak".into())),
            },
        }
    }
}

impl AppSource for FlatpakApps {
    fn kind(&self) -> SourceKind {
        SourceKind::Flatpak
    }

    fn list(&self) -> Result<Vec<AppEntry>, SourceError> {
        let apps = self.apps.clone()?;
        Ok(apps
            .into_iter()
            .map(|a| AppEntry {
                id: a.application_id,
                name: a.name,
                kind: SourceKind::Flatpak,
                scope: if a.installation == "user" {
                    Scope::User
                } else {
                    Scope::System
                },
                version: None,
                size_bytes: a.size_bytes,
                required_by: Vec::new(),
            })
            .collect())
    }

    /// The app itself. Which unused runtimes it would leave behind is the junk scanner's job.
    fn removal_plan(&self, app: &AppEntry) -> Result<RemovalPlan, SourceError> {
        if !is_plain_name(&app.id) {
            return Err(SourceError::BadName(app.id.clone()));
        }
        let user = app.scope == Scope::User;
        Ok(RemovalPlan {
            id: app.id.clone(),
            kind: SourceKind::Flatpak,
            items: vec![PlanItem {
                label: app.id.clone(),
                bytes: app.size_bytes,
            }],
            blocked: None,
            needs_admin: !user,
            to_trash: false,
            command: [
                "flatpak",
                "uninstall",
                if user { "--user" } else { "--system" },
                "--",
                app.id.as_str(),
            ]
            .map(String::from)
            .to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/flatpak/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    fn app(id: &str, name: &str, size: Option<u64>, installation: &str) -> FlatpakApp {
        FlatpakApp {
            application_id: id.into(),
            name: name.into(),
            size_bytes: size,
            installation: installation.into(),
        }
    }

    #[test]
    fn real_output() {
        assert_eq!(
            parse_list(&fixture("list-real.txt")).unwrap(),
            [app(
                "org.vinegarhq.Sober",
                "Sober",
                Some(18_100_000),
                "system"
            )]
        );
    }

    #[test]
    fn units_are_decimal() {
        assert_eq!(
            parse_list(&fixture("list-handwritten.txt")).unwrap(),
            [
                app("org.gimp.GIMP", "GIMP", Some(1_200_000_000), "system"),
                app("org.mozilla.firefox", "Firefox", Some(310_500_000), "user"),
                app("com.example.Tiny", "Tiny Tool", Some(532), "user"),
            ]
        );
    }

    #[test]
    fn comma_decimals_and_non_ascii_names() {
        assert_eq!(
            parse_list(&fixture("list-handwritten-de.txt")).unwrap(),
            [
                app("org.gimp.GIMP", "GIMP", Some(1_200_000_000), "system"),
                app(
                    "org.kde.kate",
                    "Kate – Texteditor",
                    Some(84_700_000),
                    "system"
                ),
            ]
        );
    }

    #[test]
    fn broken_line_is_an_error() {
        assert_eq!(
            parse_list(&fixture("list-broken.txt")),
            Err(ParseError::WrongColumnCount { line: 1, found: 2 })
        );
    }

    #[test]
    fn empty_output_is_no_apps() {
        assert_eq!(parse_list(""), Ok(vec![]));
        assert_eq!(parse_list("\n\n"), Ok(vec![]));
    }

    #[test]
    fn unreadable_size_is_unknown_not_fatal() {
        let apps = parse_list("a.b.C\tC\t<1 kB?\tuser\n").unwrap();
        assert_eq!(apps, [app("a.b.C", "C", None, "user")]);
    }

    fn junk_fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/junk/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    fn rt(reference: &str, size: Option<u64>, installation: &str) -> Runtime {
        Runtime {
            reference: reference.into(),
            size_bytes: size,
            installation: installation.into(),
        }
    }

    #[test]
    fn real_runtimes_parse() {
        let runtimes = parse_runtimes(&junk_fixture("flatpak-runtimes-real.txt")).unwrap();
        assert_eq!(runtimes.len(), 6);
        assert_eq!(
            runtimes[4],
            rt(
                "org.gnome.Platform/x86_64/51",
                runtimes[4].size_bytes,
                "system"
            )
        );
        assert!(runtimes[4].size_bytes.unwrap() > 100_000_000);
    }

    #[test]
    fn real_app_runtimes_parse() {
        assert_eq!(
            parse_app_runtimes(&junk_fixture("flatpak-app-runtimes-real.txt")).unwrap(),
            [AppRuntime {
                application_id: "org.vinegarhq.Sober".into(),
                runtime: "org.gnome.Platform/x86_64/51".into(),
                installation: "system".into(),
            }]
        );
    }

    #[test]
    fn runtime_lines_with_too_few_columns_are_an_error() {
        assert_eq!(
            parse_runtimes(&junk_fixture("flatpak-runtimes-broken.txt")),
            Err(ParseError::BadRuntimeLine { line: 1 })
        );
        assert_eq!(
            parse_runtimes("no-slashes\t1 MB\tuser\n"),
            Err(ParseError::BadRuntimeLine { line: 1 })
        );
    }

    #[test]
    fn real_machine_has_nothing_unused() {
        let runtimes = parse_runtimes(&junk_fixture("flatpak-runtimes-real.txt")).unwrap();
        let apps = parse_app_runtimes(&junk_fixture("flatpak-app-runtimes-real.txt")).unwrap();
        let result = unused_runtimes(&runtimes, &apps);
        assert_eq!(result.unused, []);
        // Extensions of a used runtime must never be offered, even though no app names them.
        assert!(result.kept.iter().any(|(r, _)| r
            .reference
            .starts_with("org.freedesktop.Platform.GL.default")));
    }

    #[test]
    fn unused_base_runtimes_are_found_and_extensions_and_freedesktop_are_kept() {
        let runtimes = parse_runtimes(&junk_fixture("flatpak-runtimes-handwritten.txt")).unwrap();
        let apps =
            parse_app_runtimes(&junk_fixture("flatpak-app-runtimes-handwritten.txt")).unwrap();
        let result = unused_runtimes(&runtimes, &apps);
        let unused: Vec<(&str, u64)> = result
            .unused
            .iter()
            .map(|r| (r.reference.as_str(), r.size_bytes.unwrap()))
            .collect();
        assert_eq!(
            unused,
            [
                ("org.gnome.Platform/x86_64/47", 1_100_000_000),
                ("org.kde.Platform/x86_64/6.7", 900_000_000),
                ("org.kde.Sdk/x86_64/6.7", 2_000_000_000),
            ]
        );
        assert!(result
            .kept
            .iter()
            .any(|(r, _)| r.reference == "org.freedesktop.Platform/x86_64/23.08"));
        assert!(result
            .kept
            .iter()
            .any(|(r, _)| r.reference.starts_with("org.gnome.Platform.Locale")));
    }

    #[test]
    fn a_runtime_used_from_another_installation_is_used() {
        let runtimes = vec![rt("org.kde.Platform/x86_64/6.7", Some(1), "system")];
        let apps = vec![AppRuntime {
            application_id: "a.b.C".into(),
            runtime: "org.kde.Platform/x86_64/6.7".into(),
            installation: "user".into(),
        }];
        assert_eq!(unused_runtimes(&runtimes, &apps).unused, []);
    }

    #[test]
    fn flatpak_apps_become_entries_with_scope_and_size() {
        let src = FlatpakApps::from_list(&fixture("list-handwritten.txt"));
        let apps = src.list().unwrap();
        assert_eq!(apps.len(), 3);
        let gimp = &apps[0];
        assert_eq!(
            (gimp.id.as_str(), gimp.name.as_str()),
            ("org.gimp.GIMP", "GIMP")
        );
        assert_eq!(gimp.kind, SourceKind::Flatpak);
        assert_eq!(gimp.scope, crate::source::Scope::System);
        assert_eq!(gimp.size_bytes, Some(1_200_000_000));
        assert_eq!(apps[1].scope, crate::source::Scope::User);
    }

    #[test]
    fn user_flatpaks_need_no_admin_and_system_ones_do() {
        let src = FlatpakApps::from_list(&fixture("list-handwritten.txt"));
        let apps = src.list().unwrap();
        let user = src.removal_plan(&apps[1]).unwrap();
        assert!(!user.needs_admin && !user.to_trash && user.blocked.is_none());
        assert_eq!(
            user.command,
            [
                "flatpak",
                "uninstall",
                "--user",
                "--",
                "org.mozilla.firefox"
            ]
        );
        let system = src.removal_plan(&apps[0]).unwrap();
        assert!(system.needs_admin);
        assert_eq!(
            system.command,
            ["flatpak", "uninstall", "--system", "--", "org.gimp.GIMP"]
        );
    }

    #[test]
    fn broken_flatpak_output_is_a_source_error() {
        let src = FlatpakApps::from_list(&fixture("list-broken.txt"));
        assert!(matches!(src.list(), Err(SourceError::Parse(_))));
    }

    #[test]
    fn german_flatpak_output_lists_too() {
        let src = FlatpakApps::from_list(&fixture("list-handwritten-de.txt"));
        assert!(!src.list().unwrap().is_empty());
    }

    #[test]
    fn a_hostile_flatpak_id_never_reaches_a_plan() {
        let src = FlatpakApps::from_list(&fixture("list-handwritten.txt"));
        let mut app = src.list().unwrap().remove(0);
        for bad in ["--all", "x y", "../x", ""] {
            app.id = bad.into();
            assert!(
                matches!(src.removal_plan(&app), Err(SourceError::BadName(_))),
                "{bad:?}"
            );
        }
    }
}
