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

use crate::source::{AppEntry, AppSource, PlanItem, RemovalPlan, Scope, SourceError, SourceKind};

/// AppImages in the usual folders as an app source.
pub struct AppImageApps {
    dirs: Vec<PathBuf>,
    /// Where the user's launchers live and where their icons may be; when set, a removal plan also
    /// lists the launcher and icon that belong to the AppImage.
    menu: Option<(PathBuf, Vec<PathBuf>)>,
}

impl AppImageApps {
    pub fn in_dirs(dirs: Vec<PathBuf>) -> Self {
        AppImageApps { dirs, menu: None }
    }

    /// Also remove the AppImage's launcher and icon: `applications_dir` and `icon_dirs` under the
    /// user's data folder.
    pub fn with_menu(mut self, applications_dir: PathBuf, icon_dirs: Vec<PathBuf>) -> Self {
        self.menu = Some((applications_dir, icon_dirs));
        self
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
        Ok(find_appimages(&self.dirs)
            .into_iter()
            .map(|a| AppEntry {
                id: a.path.display().to_string(),
                name: a.name,
                kind: SourceKind::AppImage,
                scope: Scope::User,
                version: None,
                size_bytes: Some(a.size_bytes),
                required_by: Vec::new(),
            })
            .collect())
    }

    /// Moves the file to the Trash. Only a file that is found in the searched folders right now
    /// can be planned, so an id cannot point at some other file.
    fn removal_plan(&self, app: &AppEntry) -> Result<RemovalPlan, SourceError> {
        let current = find_appimages(&self.dirs);
        let Some(found) = current
            .iter()
            .find(|a| a.path.display().to_string() == app.id)
        else {
            return Err(SourceError::BadName(app.id.clone()));
        };
        let mut items = vec![PlanItem {
            label: app.id.clone(),
            bytes: Some(found.size_bytes),
        }];
        if let Some((applications_dir, icon_dirs)) = &self.menu {
            for entry in menu_entries(&found.path, applications_dir, icon_dirs) {
                for path in std::iter::once(entry.desktop_file).chain(entry.icon) {
                    items.push(PlanItem {
                        bytes: fs::symlink_metadata(&path).ok().map(|m| m.len()),
                        label: path.display().to_string(),
                    });
                }
            }
        }
        Ok(RemovalPlan {
            id: app.id.clone(),
            kind: SourceKind::AppImage,
            items,
            blocked: None,
            needs_admin: false,
            to_trash: true,
            command: Vec::new(),
        })
    }
}

/// A launcher the AppImage put in the user's menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub desktop_file: PathBuf,
    pub icon: Option<PathBuf>,
}

/// The `.desktop` files in `applications_dir` whose `Exec=` starts exactly `appimage`, and each one's
/// icon when it is a file inside one of `icon_dirs`. Nothing outside those folders is ever named.
pub fn menu_entries(
    appimage: &Path,
    applications_dir: &Path,
    icon_dirs: &[PathBuf],
) -> Vec<MenuEntry> {
    let Ok(read) = fs::read_dir(applications_dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "desktop"))
        .collect();
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let Some(entry) = crate::desktop::parse_desktop_file(&text) else {
            continue;
        };
        if crate::desktop::exec_program(&entry.exec)
            .as_deref()
            .map(Path::new)
            != Some(appimage)
        {
            continue;
        }
        let icon = entry
            .icon
            .as_deref()
            .and_then(|name| find_icon(name, icon_dirs));
        found.push(MenuEntry {
            desktop_file: file,
            icon,
        });
    }
    found
}

fn has_dotdot(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// An icon file inside `icon_dirs`: a path that is inside one, or a name looked up below them.
fn find_icon(name: &str, icon_dirs: &[PathBuf]) -> Option<PathBuf> {
    let as_path = Path::new(name);
    if as_path.is_absolute() {
        let inside = !has_dotdot(as_path) && icon_dirs.iter().any(|d| as_path.starts_with(d));
        let is_file = fs::symlink_metadata(as_path).is_ok_and(|m| m.is_file());
        return (inside && is_file).then(|| as_path.to_path_buf());
    }
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return None;
    }
    icon_dirs.iter().find_map(|dir| search_icon(dir, name, 6))
}

fn search_icon(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(dir).ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in &entries {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_file()
            && path.file_stem().is_some_and(|s| s == name)
            && path
                .extension()
                .is_some_and(|x| matches!(x.to_str(), Some("png" | "svg" | "xpm")))
        {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    entries
        .iter()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .find_map(|e| search_icon(&e.path(), name, depth - 1))
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

    fn menu_world() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let home = tempfile::tempdir().unwrap();
        let apps = home.path().join(".local/share/applications");
        let icons = home.path().join(".local/share/icons");
        fs::create_dir_all(&apps).unwrap();
        let image = home.path().join("Applications/Krita 5.2.AppImage");
        write(&image, 10);
        let fx = format!(
            "{}/../../tests/fixtures/appimage-menu",
            env!("CARGO_MANIFEST_DIR")
        );
        let krita = fs::read_to_string(format!("{fx}/krita.desktop")).unwrap();
        fs::write(
            apps.join("krita.desktop"),
            krita.replace("/home/u", home.path().to_str().unwrap()),
        )
        .unwrap();
        fs::copy(format!("{fx}/other.desktop"), apps.join("other.desktop")).unwrap();
        (home, image, apps, icons)
    }

    #[test]
    fn a_launcher_whose_exec_is_the_appimage_is_found_with_its_icon() {
        let (_home, image, apps, icons) = menu_world();
        write(&icons.join("hicolor/256x256/apps/krita.png"), 5);
        write(&icons.join("hicolor/256x256/apps/other.png"), 5);
        let found = menu_entries(&image, &apps, std::slice::from_ref(&icons));
        assert_eq!(
            found,
            [MenuEntry {
                desktop_file: apps.join("krita.desktop"),
                icon: Some(icons.join("hicolor/256x256/apps/krita.png")),
            }]
        );
    }

    #[test]
    fn a_missing_icon_or_icon_folder_is_fine() {
        let (_home, image, apps, icons) = menu_world();
        let found = menu_entries(&image, &apps, &[icons]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].icon, None);
        assert!(menu_entries(&image, &apps.join("nope"), &[]).is_empty());
    }

    #[test]
    fn an_icon_path_outside_the_icon_folders_is_never_named() {
        let (home, image, apps, icons) = menu_world();
        let secret = home.path().join("secret.png");
        write(&secret, 5);
        let text = fs::read_to_string(apps.join("krita.desktop")).unwrap();
        fs::write(
            apps.join("krita.desktop"),
            text.replace("Icon=krita", &format!("Icon={}", secret.display())),
        )
        .unwrap();
        assert_eq!(
            menu_entries(&image, &apps, std::slice::from_ref(&icons))[0].icon,
            None
        );
        let inside = icons.join("krita.png");
        write(&inside, 5);
        let text = fs::read_to_string(apps.join("krita.desktop")).unwrap();
        fs::write(
            apps.join("krita.desktop"),
            text.replace(
                &format!("Icon={}", secret.display()),
                &format!("Icon={}", inside.display()),
            ),
        )
        .unwrap();
        assert_eq!(menu_entries(&image, &apps, &[icons])[0].icon, Some(inside));
    }

    #[test]
    fn dotdot_icon_names_and_launchers_for_other_programs_are_ignored() {
        let (home, image, apps, icons) = menu_world();
        write(&home.path().join("evil.png"), 5);
        let text = fs::read_to_string(apps.join("krita.desktop")).unwrap();
        fs::write(
            apps.join("krita.desktop"),
            text.replace("Icon=krita", "Icon=../../../evil"),
        )
        .unwrap();
        assert_eq!(menu_entries(&image, &apps, &[icons])[0].icon, None);
        let other = home.path().join("Applications/Other.AppImage");
        assert!(menu_entries(&other, &apps, &[]).is_empty());
    }

    #[test]
    fn the_plan_also_lists_the_launcher_and_icon_of_the_appimage() {
        let (home, image, apps, icons) = menu_world();
        write(&icons.join("hicolor/256x256/apps/krita.png"), 5);
        let src = AppImageApps::in_dirs(vec![home.path().join("Applications")])
            .with_menu(apps.clone(), vec![icons.clone()]);
        let app = src.list().unwrap().remove(0);
        let plan = src.removal_plan(&app).unwrap();
        let labels: Vec<_> = plan.items.iter().map(|i| i.label.clone()).collect();
        assert_eq!(
            labels,
            [
                image.display().to_string(),
                apps.join("krita.desktop").display().to_string(),
                icons
                    .join("hicolor/256x256/apps/krita.png")
                    .display()
                    .to_string(),
            ]
        );
        assert!(plan.to_trash && !plan.needs_admin);
        assert!(plan.items.iter().all(|i| i.bytes.is_some()));
    }

    #[test]
    fn without_menu_folders_the_plan_is_just_the_file() {
        let (home, _image, _apps, _icons) = menu_world();
        let src = AppImageApps::in_dirs(vec![home.path().join("Applications")]);
        let app = src.list().unwrap().remove(0);
        assert_eq!(src.removal_plan(&app).unwrap().items.len(), 1);
    }
}
