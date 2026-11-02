//! Junk scanner, read-only and as the user. Finds what a clean-up could remove and how much each
//! piece weighs. The result is the exact list the confirm screen will show; nothing is removed here.
//!
//! Sources: app caches in `~/.cache`, the Trash, package caches, the journal, and unused Flatpak
//! runtimes. Everything outside `~/.cache` and the Trash needs admin access to clean, so those items
//! carry `needs_admin`.

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use tidewise_core::scan::{self, Progress, ScanOptions};

use crate::{flatpak, journal};

const JOURNAL_LABEL: &str = "System log (journal)";
const RUNTIMES_LABEL: &str = "Unused Flatpak runtimes";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JunkKind {
    AppCache,
    /// An app cache that takes a long download to come back (see `cachekind`).
    SlowCache,
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
        self.items
            .iter()
            .filter(|i| !i.needs_admin)
            .map(|i| i.bytes)
            .sum()
    }

    /// Bytes that need admin access.
    pub fn admin_bytes(&self) -> u64 {
        self.items
            .iter()
            .filter(|i| i.needs_admin)
            .map(|i| i.bytes)
            .sum()
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
    /// Things that could not be looked at at all (a command failed), shown with the skipped ones.
    pub unavailable: Vec<Skipped>,
}

pub fn scan(sources: &Sources) -> JunkReport {
    let mut report = JunkReport {
        items: Vec::new(),
        skipped: sources.unavailable.clone(),
    };
    if let Some(cache) = &sources.cache_dir {
        app_caches(cache, &sources.running, &mut report);
    }
    if let Some(trash) = &sources.trash_dir {
        let bytes = dir_bytes(trash).0;
        if bytes > 0 {
            report.items.push(JunkItem {
                kind: JunkKind::Trash,
                label: "Trash".into(),
                path: Some(trash.clone()),
                bytes,
                needs_admin: false,
            });
        }
    }
    for (label, dir) in &sources.package_caches {
        if !dir.is_dir() {
            continue; // that package manager is not installed
        }
        let (bytes, unreadable) = dir_bytes(dir);
        if bytes > 0 {
            report.items.push(JunkItem {
                kind: JunkKind::PackageCache,
                label: label.clone(),
                path: Some(dir.clone()),
                bytes,
                needs_admin: true,
            });
        } else if unreadable > 0 {
            report.skipped.push(Skipped {
                label: label.clone(),
                reason: "Tidewise is not allowed to look inside it".into(),
            });
        }
    }
    if let Some(text) = &sources.journal_text {
        match journal::parse_disk_usage(text) {
            Ok(bytes) if bytes > 0 => report.items.push(JunkItem {
                kind: JunkKind::Journal,
                label: JOURNAL_LABEL.into(),
                path: None,
                bytes,
                needs_admin: true,
            }),
            Ok(_) => {}
            Err(_) => report.skipped.push(Skipped {
                label: JOURNAL_LABEL.into(),
                reason: "Tidewise could not understand what journalctl printed".into(),
            }),
        }
    }
    flatpak_runtimes(sources, &mut report);
    report.items.sort_by_key(|i| std::cmp::Reverse(i.bytes));
    report
}

/// Bytes on disk (like `du`) and how many things could not be read.
fn dir_bytes(dir: &Path) -> (u64, u64) {
    let options = ScanOptions {
        biggest: 0,
        ..ScanOptions::default()
    };
    let result = scan::scan(dir, &options, &Progress::default(), &AtomicBool::new(false));
    (result.total_bytes, result.skipped)
}

fn app_caches(cache: &Path, running: &[String], report: &mut JunkReport) {
    let Ok(entries) = fs::read_dir(cache) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let label = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            report.skipped.push(Skipped {
                label,
                reason: "It is a link, so it is left alone".into(),
            });
            continue;
        }
        if is_running(&label, running) {
            report.skipped.push(Skipped {
                label,
                reason: "The app is running right now".into(),
            });
            continue;
        }
        let bytes = if meta.is_dir() {
            dir_bytes(&path).0
        } else {
            meta.blocks() * 512
        };
        if bytes > 0 {
            let kind = match crate::cachekind::classify(&label, bytes) {
                crate::cachekind::CacheKind::Rebuilds => JunkKind::AppCache,
                crate::cachekind::CacheKind::SlowToDownload => JunkKind::SlowCache,
            };
            report.items.push(JunkItem {
                kind,
                label,
                path: Some(path),
                bytes,
                needs_admin: false,
            });
        }
    }
}

/// Does a running program belong to this cache folder? A guess that errs towards "running":
/// the names are equal, or one starts with the other (a process name is cut at 15 characters) when
/// the shorter one has at least 5 characters, or the folder is a known alias.
pub(crate) fn is_running(cache_name: &str, running: &[String]) -> bool {
    let normal = |s: &str| s.to_lowercase().replace([' ', '_'], "-");
    let name = normal(cache_name);
    let name = match name.as_str() {
        "mozilla" => "firefox".to_string(),
        _ => name,
    };
    running.iter().map(|r| normal(r)).any(|r| {
        if r.is_empty() {
            return false;
        }
        let shorter = name.len().min(r.len());
        name == r || (shorter >= 5 && (name.starts_with(&r) || r.starts_with(&name)))
    })
}

fn flatpak_runtimes(sources: &Sources, report: &mut JunkReport) {
    let (runtimes, apps) = match (&sources.flatpak_runtimes, &sources.flatpak_apps) {
        (None, None) => return,
        (Some(r), Some(a)) => (r, a),
        _ => {
            report.skipped.push(Skipped {
                label: RUNTIMES_LABEL.into(),
                reason: "Flatpak could not be asked".into(),
            });
            return;
        }
    };
    let parsed = flatpak::parse_runtimes(runtimes)
        .and_then(|r| flatpak::parse_app_runtimes(apps).map(|a| flatpak::unused_runtimes(&r, &a)));
    match parsed {
        Ok(unused) => {
            for runtime in unused.unused {
                let mut parts = runtime.reference.split('/');
                let id = parts.next().unwrap_or_default();
                let branch = parts.nth(1).unwrap_or_default();
                report.items.push(JunkItem {
                    kind: JunkKind::FlatpakRuntime,
                    label: format!("{id} {branch}"),
                    path: None,
                    bytes: runtime.size_bytes.unwrap_or(0),
                    needs_admin: runtime.installation != "user",
                });
            }
        }
        Err(_) => report.skipped.push(Skipped {
            label: RUNTIMES_LABEL.into(),
            reason: "Tidewise could not understand what flatpak printed".into(),
        }),
    }
}

/// Gather the real inputs: folders, running programs and read-only commands (by absolute path,
/// no shell, `LC_ALL=C`). A command that is missing is simply not asked; one that fails is reported.
pub fn live_sources() -> Sources {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let cache_dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| h.join(".cache")));
    let mut sources = Sources {
        cache_dir,
        trash_dir: tidewise_core::trash::default_trash_dir(),
        package_caches: vec![
            (
                "Package cache (pacman)".into(),
                PathBuf::from("/var/cache/pacman/pkg"),
            ),
            (
                "Package cache (apt)".into(),
                PathBuf::from("/var/cache/apt/archives"),
            ),
            (
                "Package cache (dnf)".into(),
                PathBuf::from("/var/cache/dnf"),
            ),
        ],
        running: running_names(),
        ..Sources::default()
    };
    sources.journal_text = ask(
        "/usr/bin/journalctl",
        &["--disk-usage"],
        JOURNAL_LABEL,
        &mut sources.unavailable,
    );
    let runtimes = ask(
        "/usr/bin/flatpak",
        &["list", "--runtime", "--columns=ref,size,installation"],
        RUNTIMES_LABEL,
        &mut sources.unavailable,
    );
    let apps = ask(
        "/usr/bin/flatpak",
        &[
            "list",
            "--app",
            "--columns=application,runtime,installation",
        ],
        RUNTIMES_LABEL,
        &mut Vec::new(),
    );
    if runtimes.is_some() && apps.is_some() {
        sources.flatpak_runtimes = runtimes;
        sources.flatpak_apps = apps;
    }
    sources
}

fn ask(
    program: &str,
    args: &[&str],
    label: &str,
    unavailable: &mut Vec<Skipped>,
) -> Option<String> {
    if !Path::new(program).exists() {
        return None;
    }
    let output = Command::new(program).args(args).env("LC_ALL", "C").output();
    match output {
        Ok(o) if o.status.success() => Some(String::from_utf8_lossy(&o.stdout).into_owned()),
        _ => {
            unavailable.push(Skipped {
                label: label.into(),
                reason: "The command to look it up failed".into(),
            });
            None
        }
    }
}

/// Lower-case names of running programs: `/proc/<pid>/comm` and the first word of the command line.
pub fn running_names() -> Vec<String> {
    let mut names = HashSet::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let pid = entry.file_name();
            if !pid.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if let Ok(comm) = fs::read_to_string(entry.path().join("comm")) {
                names.insert(comm.trim().to_lowercase());
            }
            if let Ok(cmdline) = fs::read(entry.path().join("cmdline")) {
                let first = cmdline.split(|b| *b == 0).next().unwrap_or_default();
                let first = String::from_utf8_lossy(first);
                if let Some(base) = first.rsplit('/').next() {
                    names.insert(base.to_lowercase());
                }
            }
        }
    }
    names.retain(|n| !n.is_empty());
    let mut names: Vec<String> = names.into_iter().collect();
    names.sort();
    names
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
    fn slow_to_download_caches_are_listed_as_their_own_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        write(&cache.join("huggingface/hub/model.bin"), 200_000);
        write(&cache.join("thumbnails/a.png"), 200_000);
        write(&cache.join("Docker Desktop/x"), 200_000);
        write(&cache.join("whatever/x"), 200_000);
        let report = scan(&Sources {
            cache_dir: Some(cache),
            ..Sources::default()
        });
        assert_eq!(item(&report, "huggingface").kind, JunkKind::SlowCache);
        assert_eq!(item(&report, "Docker Desktop").kind, JunkKind::SlowCache);
        assert_eq!(item(&report, "thumbnails").kind, JunkKind::AppCache);
        assert_eq!(item(&report, "whatever").kind, JunkKind::AppCache);
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
