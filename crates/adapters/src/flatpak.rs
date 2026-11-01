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

pub fn parse_runtimes(text: &str) -> Result<Vec<Runtime>, ParseError> {
    let _ = text;
    Ok(Vec::new())
}

pub fn parse_app_runtimes(text: &str) -> Result<Vec<AppRuntime>, ParseError> {
    let _ = text;
    Ok(Vec::new())
}

pub fn unused_runtimes(runtimes: &[Runtime], apps: &[AppRuntime]) -> UnusedRuntimes {
    let _ = (runtimes, apps);
    UnusedRuntimes::default()
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
}
