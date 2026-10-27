//! Read-only pacman adapter. Parses the machine-readable output of `pacman -Qi`, `-Qq`
//! and `-Qdtq`, always run with `LC_ALL=C`.

use std::path::PathBuf;
use std::process::Command;

use crate::{Package, PackageSource};

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

impl PackageSource for Pacman {
    type Error = Error;

    fn installed(&self) -> Result<Vec<Package>, Error> {
        let out = self.run(&["-Qi"])?;
        if !out.status.success() {
            return Err(failed(&out));
        }
        Ok(parse_qi(&String::from_utf8_lossy(&out.stdout))?)
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
}
