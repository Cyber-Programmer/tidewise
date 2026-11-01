//! Read-only Snap adapter. Parses `snap list --all` (run with `LC_ALL=C`). Disabled revisions are
//! the old copies Snap keeps for rollback; sizes come from the `.snap` files in `/var/lib/snapd/snaps`.

use std::path::{Path, PathBuf};

use crate::source::{
    is_plain_name, AppEntry, AppSource, Blocked, PlanItem, RemovalPlan, Scope, SourceError,
    SourceKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapRevision {
    pub name: String,
    pub version: String,
    /// `3626`, or `x1` for a local install.
    pub revision: String,
    pub notes: Vec<String>,
}

impl SnapRevision {
    /// An old copy kept for rollback.
    pub fn is_disabled(&self) -> bool {
        false
    }

    /// A base snap (`core20`, …) or `snapd` itself: other snaps and the system need it.
    pub fn is_system(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("not the C-locale header of `snap list --all`")]
    Header,
    #[error("line {line}: not a snap row")]
    BadLine { line: usize },
}

/// Every revision `snap list --all` shows, in order.
pub fn parse_snap_list(text: &str) -> Result<Vec<SnapRevision>, ParseError> {
    let _ = text;
    Err(ParseError::Header)
}

/// The disabled revisions: what "remove old snap copies" would remove.
pub fn old_revisions(all: &[SnapRevision]) -> Vec<&SnapRevision> {
    let _ = all;
    Vec::new()
}

/// Snap as an app source: one entry per snap (its active revision).
pub struct SnapApps {
    revisions: Result<Vec<SnapRevision>, SourceError>,
    snap_dir: Option<PathBuf>,
}

impl SnapApps {
    pub fn from_list(text: &str) -> Self {
        let _ = text;
        SnapApps {
            revisions: Ok(Vec::new()),
            snap_dir: None,
        }
    }

    /// Read sizes from `<dir>/<name>_<rev>.snap`.
    pub fn with_snap_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.snap_dir = Some(dir.into());
        self
    }

    /// Ask the real snap (read-only). Not installed means no snaps, not an error.
    pub fn system() -> Self {
        let _ = Path::new("/usr/bin/snap");
        SnapApps {
            revisions: Ok(Vec::new()),
            snap_dir: None,
        }
    }
}

impl AppSource for SnapApps {
    fn kind(&self) -> SourceKind {
        SourceKind::Snap
    }

    fn list(&self) -> Result<Vec<AppEntry>, SourceError> {
        let _ = (&self.revisions, &self.snap_dir, Scope::System);
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
            "{}/../../tests/fixtures/snap/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn every_revision_is_read_with_its_notes() {
        let all = parse_snap_list(&fx("snap-list-all-handwritten.txt")).unwrap();
        assert_eq!(all.len(), 8);
        assert_eq!(
            (
                all[2].name.as_str(),
                all[2].revision.as_str(),
                all[2].version.as_str()
            ),
            ("firefox", "3626", "120.0.1-1")
        );
        assert_eq!(all[1].notes, ["base", "disabled"]);
        assert_eq!(all[2].notes, Vec::<String>::new(), "`-` means no notes");
        assert_eq!(all[5].revision, "x1", "a local install");
        assert_eq!(all[7].notes, ["classic"]);
    }

    #[test]
    fn disabled_revisions_are_the_old_copies() {
        let all = parse_snap_list(&fx("snap-list-all-handwritten.txt")).unwrap();
        let old: Vec<_> = old_revisions(&all)
            .iter()
            .map(|r| (r.name.as_str(), r.revision.as_str()))
            .collect();
        assert_eq!(old, [("core20", "1974"), ("firefox", "3547")]);
        assert!(all[3].is_disabled() && !all[2].is_disabled());
    }

    #[test]
    fn base_snaps_and_snapd_are_system_snaps() {
        let all = parse_snap_list(&fx("snap-list-all-handwritten.txt")).unwrap();
        let system: Vec<_> = all
            .iter()
            .filter(|r| r.is_system())
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(system, ["core20", "core20", "snapd"]);
    }

    #[test]
    fn nothing_installed_is_an_empty_list() {
        assert_eq!(
            parse_snap_list(&fx("snap-list-none-handwritten.txt")),
            Ok(vec![])
        );
        assert_eq!(parse_snap_list(""), Ok(vec![]));
    }

    #[test]
    fn other_languages_short_rows_and_garbage_are_refused() {
        assert_eq!(
            parse_snap_list(&fx("snap-list-de-handwritten.txt")),
            Err(ParseError::Header)
        );
        assert_eq!(
            parse_snap_list("firefox 1 2 - - -\n"),
            Err(ParseError::Header)
        );
        assert_eq!(
            parse_snap_list(&fx("snap-list-broken-handwritten.txt")),
            Err(ParseError::BadLine { line: 2 })
        );
        assert_eq!(
            parse_snap_list(&fx("snap-list-garbage-handwritten.txt")),
            Err(ParseError::BadLine { line: 2 })
        );
        let head = "Name Version Rev Tracking Publisher Notes\n";
        for bad in ["a b c d e f g\n", "a/b 1 2 - - -\n", "Firefox 1 2 - - -\n"] {
            assert!(parse_snap_list(&format!("{head}{bad}")).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn one_entry_per_snap_with_the_active_revision() {
        let apps = SnapApps::from_list(&fx("snap-list-all-handwritten.txt"))
            .list()
            .unwrap();
        let ids: Vec<_> = apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "core20",
                "firefox",
                "gnome-3-38-2004",
                "my-local-tool",
                "snapd",
                "code"
            ]
        );
        let ff = apps.iter().find(|a| a.id == "firefox").unwrap();
        assert_eq!((ff.kind, ff.scope), (SourceKind::Snap, Scope::System));
        assert_eq!(ff.version.as_deref(), Some("120.0.1-1"));
        assert_eq!(ff.size_bytes, None, "no snap folder given");
    }

    #[test]
    fn sizes_come_from_the_snap_files_of_the_active_revision() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("firefox_3626.snap"), vec![0u8; 5000]).unwrap();
        std::fs::write(dir.path().join("firefox_3547.snap"), vec![0u8; 4000]).unwrap();
        let src =
            SnapApps::from_list(&fx("snap-list-all-handwritten.txt")).with_snap_dir(dir.path());
        let apps = src.list().unwrap();
        assert_eq!(
            apps.iter().find(|a| a.id == "firefox").unwrap().size_bytes,
            Some(5000)
        );
        assert_eq!(
            apps.iter().find(|a| a.id == "snapd").unwrap().size_bytes,
            None,
            "no file, no size"
        );
    }

    #[test]
    fn a_broken_listing_is_a_source_error() {
        assert!(matches!(
            SnapApps::from_list(&fx("snap-list-broken-handwritten.txt")).list(),
            Err(SourceError::Parse(_))
        ));
    }

    fn app(id: &str) -> (SnapApps, AppEntry) {
        let src = SnapApps::from_list(&fx("snap-list-all-handwritten.txt"));
        let a = src
            .list()
            .unwrap()
            .into_iter()
            .find(|a| a.id == id)
            .unwrap();
        (src, a)
    }

    #[test]
    fn removing_a_snap_lists_every_revision_with_the_old_copies_marked() {
        let (src, ff) = app("firefox");
        let plan = src.removal_plan(&ff).unwrap();
        let labels: Vec<_> = plan.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["firefox 3626", "firefox 3547 (old copy)"]);
        assert!(plan.needs_admin && plan.blocked.is_none() && !plan.to_trash);
        assert_eq!(plan.command, ["snap", "remove", "firefox"]);
    }

    #[test]
    fn base_snaps_and_snapd_are_protected() {
        for id in ["core20", "snapd"] {
            let (src, a) = app(id);
            let plan = src.removal_plan(&a).unwrap();
            assert_eq!(
                plan.blocked,
                Some(Blocked::Protected(vec![id.to_string()])),
                "{id}"
            );
        }
    }

    #[test]
    fn hostile_ids_and_unknown_snaps_get_no_plan() {
        let (src, mut a) = app("firefox");
        for bad in ["--purge", "a b", "", "../x", "nosuchsnap"] {
            a.id = bad.into();
            assert!(src.removal_plan(&a).is_err(), "{bad:?}");
        }
    }
}
