//! One list of apps across every source (pacman, apt, dnf, Flatpak, Snap, AppImage), and the dry-run
//! "what would removing this do" for each. Read-only: nothing here removes anything.
//!
//! Removing is a separate step: AppImages and user Flatpaks are removed by the app itself behind the
//! confirm screen, everything else needs the admin helper. Until a source can do it, `remove`
//! answers `NotYet`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Pacman,
    Apt,
    Dnf,
    Flatpak,
    Snap,
    AppImage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    System,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    /// What the source calls it: package name, Flatpak application id, AppImage path.
    pub id: String,
    /// What the user sees.
    pub name: String,
    pub kind: SourceKind,
    pub scope: Scope,
    pub version: Option<String>,
    pub size_bytes: Option<u64>,
    /// Packages that need this one. Empty when nothing does.
    pub required_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// Other installed software needs it.
    RequiredBy(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    /// A package name, runtime or file path.
    pub label: String,
    pub bytes: Option<u64>,
}

/// What removing one app would do, shown before anything happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalPlan {
    pub id: String,
    pub kind: SourceKind,
    pub items: Vec<PlanItem>,
    pub blocked: Option<Blocked>,
    pub needs_admin: bool,
    /// Removed by moving to the Trash (files the user could have made), not deleted.
    pub to_trash: bool,
    /// The command, for "Show technical details". Display only; never run through a shell.
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    #[error("could not run {0}")]
    Run(String),
    #[error("could not understand the output of {0}")]
    Parse(String),
    #[error("`{0}` is not a plain app name")]
    BadName(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RemoveError {
    #[error("this source cannot remove apps yet")]
    NotYet,
}

pub trait AppSource {
    fn kind(&self) -> SourceKind;

    /// Everything this source has installed.
    fn list(&self) -> Result<Vec<AppEntry>, SourceError>;

    /// Disk space the app takes, when the source knows.
    fn size(&self, app: &AppEntry) -> Option<u64> {
        app.size_bytes
    }

    /// The dry run: what would go, whether it is blocked, whether it needs admin access.
    fn removal_plan(&self, app: &AppEntry) -> Result<RemovalPlan, SourceError>;

    /// Carry out a plan that is not blocked. No source does yet.
    fn remove(&self, plan: &RemovalPlan) -> Result<(), RemoveError> {
        let _ = plan;
        Err(RemoveError::NotYet)
    }
}

/// The merged list, and which sources could not be read (the others are still listed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub apps: Vec<AppEntry>,
    pub errors: Vec<(SourceKind, String)>,
}

pub fn list_all(sources: &[&dyn AppSource]) -> Listing {
    let _ = sources;
    Listing::default()
}

/// A name that is safe to put on a command line after `--`: not empty, no leading `-`, no
/// whitespace or control characters, no `/` (AppImage ids are paths and are checked elsewhere).
pub fn is_plain_name(name: &str) -> bool {
    let _ = name;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(SourceKind, Result<Vec<AppEntry>, SourceError>);

    impl AppSource for Fixed {
        fn kind(&self) -> SourceKind {
            self.0
        }
        fn list(&self) -> Result<Vec<AppEntry>, SourceError> {
            self.1.clone()
        }
        fn removal_plan(&self, app: &AppEntry) -> Result<RemovalPlan, SourceError> {
            Err(SourceError::BadName(app.id.clone()))
        }
    }

    fn entry(name: &str, kind: SourceKind) -> AppEntry {
        AppEntry {
            id: name.into(),
            name: name.into(),
            kind,
            scope: Scope::System,
            version: None,
            size_bytes: Some(5),
            required_by: vec![],
        }
    }

    #[test]
    fn sources_merge_into_one_list_sorted_by_name_ignoring_case() {
        let a = Fixed(
            SourceKind::Pacman,
            Ok(vec![
                entry("vlc", SourceKind::Pacman),
                entry("Bash", SourceKind::Pacman),
            ]),
        );
        let b = Fixed(
            SourceKind::Flatpak,
            Ok(vec![
                entry("GIMP", SourceKind::Flatpak),
                entry("audacity", SourceKind::Flatpak),
            ]),
        );
        let listing = list_all(&[&a, &b]);
        let names: Vec<_> = listing.apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["audacity", "Bash", "GIMP", "vlc"]);
        assert!(listing.errors.is_empty());
    }

    #[test]
    fn a_source_that_fails_is_named_and_the_others_still_list() {
        let good = Fixed(
            SourceKind::Pacman,
            Ok(vec![entry("vlc", SourceKind::Pacman)]),
        );
        let bad = Fixed(
            SourceKind::Flatpak,
            Err(SourceError::Parse("flatpak".into())),
        );
        let listing = list_all(&[&bad, &good]);
        assert_eq!(listing.apps.len(), 1);
        assert_eq!(listing.errors.len(), 1);
        assert_eq!(listing.errors[0].0, SourceKind::Flatpak);
        assert!(listing.errors[0].1.contains("flatpak"));
    }

    #[test]
    fn no_sources_is_an_empty_listing() {
        assert_eq!(list_all(&[]), Listing::default());
    }

    #[test]
    fn size_comes_from_the_entry_and_remove_is_not_yet() {
        let s = Fixed(SourceKind::Pacman, Ok(vec![]));
        let e = entry("vlc", SourceKind::Pacman);
        assert_eq!(s.size(&e), Some(5));
        let plan = RemovalPlan {
            id: "vlc".into(),
            kind: SourceKind::Pacman,
            items: vec![],
            blocked: None,
            needs_admin: true,
            to_trash: false,
            command: vec![],
        };
        assert_eq!(s.remove(&plan), Err(RemoveError::NotYet));
    }

    #[test]
    fn sources_can_be_used_as_trait_objects() {
        let boxed: Vec<Box<dyn AppSource>> = vec![Box::new(Fixed(SourceKind::Snap, Ok(vec![])))];
        assert_eq!(boxed[0].kind(), SourceKind::Snap);
    }

    #[test]
    fn plain_names_are_accepted() {
        for ok in [
            "vlc",
            "lib32-glibc",
            "org.gimp.GIMP",
            "c++filt",
            "python3.12",
            "a",
        ] {
            assert!(is_plain_name(ok), "{ok}");
        }
    }

    #[test]
    fn hostile_names_are_refused() {
        let long = "a".repeat(300);
        for bad in [
            "",
            "-Rns",
            "--force",
            "-",
            "a b",
            "a\nb",
            "a\tb",
            "a\0b",
            "../x",
            "a/b",
            "$(id)",
            "a;b",
            "a|b",
            "a`b",
            "a&b",
            "*",
            "a*",
            "?",
            "ünï",
            "\u{202e}vlc",
            &long,
        ] {
            assert!(!is_plain_name(bad), "{bad:?}");
        }
    }
}
