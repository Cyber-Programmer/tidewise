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
}
