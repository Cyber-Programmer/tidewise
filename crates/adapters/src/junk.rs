//! Junk scanner, read-only and as the user. Finds what a clean-up could remove and how much each
//! piece weighs. The result is the exact list the confirm screen will show; nothing is removed here.
//!
//! Sources: app caches in `~/.cache`, the Trash, package caches, the journal, and unused Flatpak
//! runtimes. Everything outside `~/.cache` and the Trash needs admin access to clean, so those items
//! carry `needs_admin`.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JunkKind {
    AppCache,
    Trash,
    PackageCache,
    Journal,
    FlatpakRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkItem {
    pub kind: JunkKind,
    /// Plain-language name for the confirm list.
    pub label: String,
    /// Where it lives, when it is a path (not for the journal or a runtime).
    pub path: Option<PathBuf>,
    pub bytes: u64,
    /// Cleaning it needs the admin helper (system files).
    pub needs_admin: bool,
}

/// Something we looked at and left out, with the reason shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JunkReport {
    /// Biggest first.
    pub items: Vec<JunkItem>,
    pub skipped: Vec<Skipped>,
}

impl JunkReport {
    /// Bytes the user can clean without admin access.
    pub fn user_bytes(&self) -> u64 {
        0
    }

    /// Bytes that need admin access.
    pub fn admin_bytes(&self) -> u64 {
        0
    }
}

/// Everything the scan reads, so tests can feed it temp folders and saved command output.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    /// `~/.cache`.
    pub cache_dir: Option<PathBuf>,
    /// The Trash folder (the one that holds `files/` and `info/`).
    pub trash_dir: Option<PathBuf>,
    /// Label and folder of each package cache (pacman, apt, dnf).
    pub package_caches: Vec<(String, PathBuf)>,
    /// Names of running programs, as `/proc` shows them.
    pub running: Vec<String>,
    /// Output of `journalctl --disk-usage`; `None` when it could not be run.
    pub journal_text: Option<String>,
    /// Output of `flatpak list --runtime --columns=ref,size,installation`.
    pub flatpak_runtimes: Option<String>,
    /// Output of `flatpak list --app --columns=application,runtime,installation`.
    pub flatpak_apps: Option<String>,
}

pub fn scan(sources: &Sources) -> JunkReport {
    let _ = sources;
    JunkReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/junk/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        fs::read_to_string(path).unwrap()
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![7u8; bytes]).unwrap();
    }

    fn item<'a>(report: &'a JunkReport, label: &str) -> &'a JunkItem {
        report
            .items
            .iter()
            .find(|i| i.label == label)
            .unwrap_or_else(|| panic!("no item {label}: {:?}", report.items))
    }

    fn skipped<'a>(report: &'a JunkReport, label: &str) -> &'a Skipped {
        report
            .skipped
            .iter()
            .find(|s| s.label == label)
            .unwrap_or_else(|| panic!("no skipped {label}: {:?}", report.skipped))
    }

    #[test]
    fn nothing_to_read_is_an_empty_report() {
        assert_eq!(scan(&Sources::default()), JunkReport::default());
    }

    #[test]
    fn caches_are_listed_and_caches_of_running_apps_are_skipped_with_a_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        write(&cache.join("alpha/data.bin"), 200_000);
        write(&cache.join("beta/data.bin"), 200_000);
        write(&cache.join("mozilla/firefox/x"), 10_000);
        write(&cache.join("loose.cache"), 50_000);
        fs::create_dir_all(tmp.path().join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("elsewhere"), cache.join("gamma")).unwrap();
        let report = scan(&Sources {
            cache_dir: Some(cache),
            running: vec!["beta".into(), "firefox".into()],
            ..Sources::default()
        });
        let alpha = item(&report, "alpha");
        assert_eq!(alpha.kind, JunkKind::AppCache);
        assert!(!alpha.needs_admin);
        assert!(alpha.bytes >= 200_000, "{}", alpha.bytes);
        assert!(item(&report, "loose.cache").bytes >= 50_000);
        assert!(report
            .items
            .iter()
            .all(|i| i.label != "beta" && i.label != "mozilla"));
        assert!(skipped(&report, "beta").reason.contains("running"));
        assert!(skipped(&report, "mozilla").reason.contains("running"));
        assert!(skipped(&report, "gamma").reason.contains("link"));
        assert!(report.items.iter().all(|i| i.label != "gamma"));
        // Biggest first.
        assert!(report.items.windows(2).all(|w| w[0].bytes >= w[1].bytes));
    }

    #[test]
    fn a_program_name_cut_to_15_characters_still_matches_its_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        write(&cache.join("google-chrome-beta/x"), 10_000);
        let report = scan(&Sources {
            cache_dir: Some(cache),
            running: vec!["google-chrome-b".into()],
            ..Sources::default()
        });
        assert!(report.items.is_empty(), "{:?}", report.items);
        assert!(skipped(&report, "google-chrome-beta")
            .reason
            .contains("running"));
    }

    #[test]
    fn a_short_unrelated_name_does_not_hide_a_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        write(&cache.join("fontconfig/x"), 10_000);
        let report = scan(&Sources {
            cache_dir: Some(cache),
            running: vec!["font".into(), "sh".into()],
            ..Sources::default()
        });
        assert_eq!(item(&report, "fontconfig").kind, JunkKind::AppCache);
    }

    #[test]
    fn trash_is_sized_and_cleanable_without_admin() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        write(&trash.join("files/old.iso"), 300_000);
        write(&trash.join("info/old.iso.trashinfo"), 100);
        let report = scan(&Sources {
            trash_dir: Some(trash.clone()),
            ..Sources::default()
        });
        let t = item(&report, "Trash");
        assert_eq!(t.kind, JunkKind::Trash);
        assert_eq!(t.path.as_deref(), Some(trash.as_path()));
        assert!(t.bytes >= 300_000);
        assert!(!t.needs_admin);
    }

    #[test]
    fn package_caches_need_admin_and_a_missing_one_is_simply_absent() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("pacman/foo.pkg.tar.zst"), 400_000);
        let report = scan(&Sources {
            package_caches: vec![
                ("Package cache (pacman)".into(), tmp.path().join("pacman")),
                ("Package cache (apt)".into(), tmp.path().join("no-apt-here")),
            ],
            ..Sources::default()
        });
        let p = item(&report, "Package cache (pacman)");
        assert_eq!(p.kind, JunkKind::PackageCache);
        assert!(p.needs_admin);
        assert!(p.bytes >= 400_000);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.skipped, []);
    }

    #[test]
    fn journal_size_comes_from_journalctl_text() {
        let report = scan(&Sources {
            journal_text: Some(fixture("journal-real.txt")),
            ..Sources::default()
        });
        let j = item(&report, "System log (journal)");
        assert_eq!(j.kind, JunkKind::Journal);
        assert_eq!(j.bytes, 3_006_477_107);
        assert!(j.needs_admin);
        assert_eq!(j.path, None);
    }

    #[test]
    fn unreadable_journal_output_is_reported_not_guessed() {
        let report = scan(&Sources {
            journal_text: Some(fixture("journal-broken.txt")),
            ..Sources::default()
        });
        assert_eq!(report.items, []);
        assert!(skipped(&report, "System log (journal)")
            .reason
            .contains("understand"));
    }

    #[test]
    fn unused_flatpak_runtimes_are_listed_with_their_installation() {
        let report = scan(&Sources {
            flatpak_runtimes: Some(fixture("flatpak-runtimes-handwritten.txt")),
            flatpak_apps: Some(fixture("flatpak-app-runtimes-handwritten.txt")),
            ..Sources::default()
        });
        let gnome = item(&report, "org.gnome.Platform 47");
        assert_eq!(gnome.kind, JunkKind::FlatpakRuntime);
        assert_eq!(gnome.bytes, 1_100_000_000);
        assert!(gnome.needs_admin, "system installation");
        assert!(
            !item(&report, "org.kde.Platform 6.7").needs_admin,
            "user installation"
        );
        assert_eq!(report.items.len(), 3);
    }

    #[test]
    fn flatpak_output_that_does_not_parse_is_reported() {
        let report = scan(&Sources {
            flatpak_runtimes: Some(fixture("flatpak-runtimes-broken.txt")),
            flatpak_apps: Some(String::new()),
            ..Sources::default()
        });
        assert_eq!(report.items, []);
        assert!(skipped(&report, "Unused Flatpak runtimes")
            .reason
            .contains("understand"));
    }

    #[test]
    fn totals_split_by_who_can_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join("Trash");
        write(&trash.join("files/a"), 100_000);
        let report = scan(&Sources {
            trash_dir: Some(trash),
            journal_text: Some(fixture("journal-real.txt")),
            ..Sources::default()
        });
        assert_eq!(report.admin_bytes(), 3_006_477_107);
        assert!(report.user_bytes() >= 100_000 && report.user_bytes() < 3_006_477_107);
    }

    #[test]
    fn scanning_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        write(&cache.join("alpha/data.bin"), 1000);
        let list = |root: &Path| -> Vec<String> {
            let mut out = Vec::new();
            let mut stack = vec![root.to_path_buf()];
            while let Some(d) = stack.pop() {
                for e in fs::read_dir(d).unwrap().flatten() {
                    out.push(format!(
                        "{} {}",
                        e.path().display(),
                        e.metadata().unwrap().len()
                    ));
                    if e.path().is_dir() {
                        stack.push(e.path());
                    }
                }
            }
            out.sort();
            out
        };
        let before = list(tmp.path());
        let _ = scan(&Sources {
            cache_dir: Some(cache),
            ..Sources::default()
        });
        assert_eq!(list(tmp.path()), before);
    }
}
