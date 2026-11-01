//! Read-only AppImage discovery: `*.AppImage` files in a few well-known folders.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppImage {
    pub path: PathBuf,
    /// File name without the `.AppImage` ending.
    pub name: String,
    pub size_bytes: u64,
}

/// The folders searched, given the user's home folder.
pub fn default_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Applications"),
        home.join(".local/bin"),
        home.join("Downloads"),
    ]
}

/// Find AppImages directly inside `dirs` (not in subfolders). Missing folders are skipped,
/// symlinks are ignored. Sorted by name.
pub fn find_appimages(dirs: &[PathBuf]) -> Vec<AppImage> {
    let mut found: Vec<AppImage> = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".AppImage"))
                .filter(|n| !n.is_empty())
            else {
                continue;
            };
            // symlink_metadata: do not follow symlinks.
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_file() || found.iter().any(|a| a.path == path) {
                continue;
            }
            found.push(AppImage {
                name: name.to_string(),
                size_bytes: meta.len(),
                path,
            });
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    found
}

use crate::source::{AppEntry, AppSource, RemovalPlan, SourceError, SourceKind};

/// AppImages in the usual folders as an app source.
pub struct AppImageApps {
    dirs: Vec<PathBuf>,
}

impl AppImageApps {
    pub fn in_dirs(dirs: Vec<PathBuf>) -> Self {
        AppImageApps { dirs }
    }

    pub fn for_home(home: &Path) -> Self {
        Self::in_dirs(default_dirs(home))
    }
}

impl AppSource for AppImageApps {
    fn kind(&self) -> SourceKind {
        SourceKind::AppImage
    }

    fn list(&self) -> Result<Vec<AppEntry>, SourceError> {
        let _ = &self.dirs;
        Ok(Vec::new())
    }

    fn removal_plan(&self, app: &AppEntry) -> Result<RemovalPlan, SourceError> {
        Err(SourceError::BadName(app.id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn default_dirs_are_the_three_documented_ones() {
        assert_eq!(
            default_dirs(Path::new("/home/me")),
            [
                PathBuf::from("/home/me/Applications"),
                PathBuf::from("/home/me/.local/bin"),
                PathBuf::from("/home/me/Downloads"),
            ]
        );
    }

    #[test]
    fn finds_appimages_with_name_and_size() {
        let home = tempfile::tempdir().unwrap();
        let dirs = default_dirs(home.path());
        write(&dirs[0].join("Krita-5.2.AppImage"), 300);
        write(&dirs[1].join("tool.AppImage"), 10);
        write(&dirs[2].join("Obsidian.AppImage"), 2000);
        let found = find_appimages(&dirs);
        let summary: Vec<_> = found
            .iter()
            .map(|a| (a.name.as_str(), a.size_bytes))
            .collect();
        assert_eq!(
            summary,
            [("Krita-5.2", 300), ("Obsidian", 2000), ("tool", 10)]
        );
        assert_eq!(found[0].path, dirs[0].join("Krita-5.2.AppImage"));
    }

    #[test]
    fn ignores_other_files_subfolders_and_symlinks() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("Applications");
        write(&dir.join("notes.txt"), 5);
        write(&dir.join("AppImage"), 5);
        write(&dir.join("deep/Nested.AppImage"), 5);
        write(&dir.join("backup.AppImage.bak"), 5);
        write(&home.path().join("real.AppImage"), 7);
        std::os::unix::fs::symlink(home.path().join("real.AppImage"), dir.join("link.AppImage"))
            .unwrap();
        fs::create_dir_all(dir.join("Folder.AppImage")).unwrap();
        assert_eq!(find_appimages(&[dir]), []);
    }

    #[test]
    fn missing_folders_and_empty_list_are_fine() {
        assert_eq!(
            find_appimages(&[PathBuf::from("/nonexistent/tidewise-test")]),
            []
        );
        assert_eq!(find_appimages(&[]), []);
    }

    #[test]
    fn same_folder_listed_twice_is_not_double_counted() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().to_path_buf();
        write(&dir.join("a.AppImage"), 1);
        assert_eq!(find_appimages(&[dir.clone(), dir]).len(), 1);
    }

    #[test]
    fn appimages_become_user_entries_with_their_path_as_id() {
        let home = tempfile::tempdir().unwrap();
        let dirs = default_dirs(home.path());
        write(&dirs[0].join("Krita-5.2.AppImage"), 300);
        let apps = AppImageApps::in_dirs(dirs.clone()).list().unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Krita-5.2");
        assert_eq!(
            apps[0].id,
            dirs[0].join("Krita-5.2.AppImage").display().to_string()
        );
        assert_eq!(apps[0].kind, SourceKind::AppImage);
        assert_eq!(apps[0].scope, crate::source::Scope::User);
        assert_eq!(apps[0].size_bytes, Some(300));
    }

    #[test]
    fn an_appimage_plan_moves_the_file_to_the_trash_without_admin() {
        let home = tempfile::tempdir().unwrap();
        let dirs = default_dirs(home.path());
        write(&dirs[1].join("tool.AppImage"), 10);
        let src = AppImageApps::in_dirs(dirs.clone());
        let app = src.list().unwrap().remove(0);
        let plan = src.removal_plan(&app).unwrap();
        assert!(plan.to_trash && !plan.needs_admin && plan.blocked.is_none());
        assert_eq!(
            plan.items[0].label,
            dirs[1].join("tool.AppImage").display().to_string()
        );
        assert_eq!(plan.items[0].bytes, Some(10));
        assert!(
            plan.command.is_empty(),
            "no command: it is moved to the Trash"
        );
    }

    #[test]
    fn a_plan_is_refused_for_a_path_that_is_not_a_listed_appimage() {
        let home = tempfile::tempdir().unwrap();
        let dirs = default_dirs(home.path());
        write(&dirs[0].join("a.AppImage"), 1);
        write(&home.path().join("Documents/thesis.odt"), 1);
        let src = AppImageApps::in_dirs(dirs);
        let mut app = src.list().unwrap().remove(0);
        for bad in [
            home.path()
                .join("Documents/thesis.odt")
                .display()
                .to_string(),
            format!(
                "{}/Applications/../Documents/thesis.odt",
                home.path().display()
            ),
            "/etc/passwd".to_string(),
            String::new(),
        ] {
            app.id = bad.clone();
            assert!(
                matches!(src.removal_plan(&app), Err(SourceError::BadName(_))),
                "{bad:?}"
            );
        }
    }
}
