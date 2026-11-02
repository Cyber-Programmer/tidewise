//! Uninstalling what Tidewise may remove itself, after the confirm screen: AppImages (the file, its
//! launcher and its icon go to the Trash) and *user* Flatpaks (through the runner). Everything else
//! needs the admin helper and answers `NeedsAdmin` without touching anything.
//!
//! The list is re-checked right before acting: the plan the user confirmed must equal a plan made
//! just now, or nothing happens.

use std::path::Path;
use std::time::SystemTime;

use tidewise_core::trash;

use crate::flatpak::{uninstall_user, UninstallError};
use crate::runner::CommandRunner;
use crate::source::{is_plain_name, AppEntry, Blocked, RemovalPlan, Scope, SourceKind};

/// The folder a user Flatpak keeps its settings and data in, when Tidewise knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsFolder {
    pub path: std::path::PathBuf,
    pub bytes: u64,
}

/// `~/.var/app/<id>` of a **user Flatpak**, when it is a real folder (not a link) and the id is a
/// plain name. Nothing else is ever guessed: not for AppImages, not for packages, not for system
/// Flatpaks.
pub fn settings_folder(app: &AppEntry, home: &Path) -> Option<SettingsFolder> {
    if app.kind != SourceKind::Flatpak
        || app.scope != Scope::User
        || !is_plain_name(&app.id)
        || app.id.starts_with('.')
    {
        return None;
    }
    let is_real_dir = |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir());
    let var = home.join(".var");
    let apps = var.join("app");
    let path = apps.join(&app.id);
    if !(is_real_dir(&var) && is_real_dir(&apps) && is_real_dir(&path)) {
        return None;
    }
    let bytes = tidewise_core::scan::scan(
        &path,
        &Default::default(),
        &Default::default(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .total_bytes;
    Some(SettingsFolder { path, bytes })
}

pub struct Env<'a> {
    /// Also move the app's settings folder (see `settings_folder`) to the Trash.
    pub delete_settings: bool,
    pub trash_dir: &'a Path,
    /// The user's home: nothing outside it is ever moved, and it is never moved itself.
    pub home: &'a Path,
    pub now: SystemTime,
    pub runner: &'a dyn CommandRunner,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Removed {
        /// What was moved to the Trash or removed, as listed in the plan.
        items: Vec<String>,
        freed: u64,
    },
    Blocked(Blocked),
    /// Needs the admin helper, which does not exist yet. Nothing was changed.
    NeedsAdmin,
    /// Stopped with a plain-language reason; `removed` is what had already gone.
    Failed {
        reason: String,
        removed: Vec<String>,
    },
}

pub fn run(app: &AppEntry, confirmed: &RemovalPlan, fresh: &RemovalPlan, env: &Env) -> Outcome {
    if let Some(blocked) = &fresh.blocked {
        return Outcome::Blocked(blocked.clone());
    }
    if confirmed.id != fresh.id || confirmed.items != fresh.items {
        return failed(
            "The list changed after you confirmed it, so nothing was removed. Please look at it again.",
            Vec::new(),
        );
    }
    match (app.kind, app.scope) {
        (SourceKind::AppImage, _) => move_to_trash(confirmed, env),
        (SourceKind::Flatpak, Scope::User) => match uninstall_user(env.runner, app) {
            Ok(()) => {
                let mut items = vec![app.id.clone()];
                let mut freed = app.size_bytes.unwrap_or(0);
                // Only after the app is gone, and only the folder Tidewise knows: to the Trash.
                if env.delete_settings {
                    if let Some(folder) = settings_folder(app, env.home) {
                        match trash::move_to_trash(&folder.path, env.trash_dir, env.home, env.now) {
                            Ok(_) => {
                                items.push(folder.path.display().to_string());
                                freed += folder.bytes;
                            }
                            Err(e) => {
                                return failed(
                                    &format!(
                                        "{} is uninstalled, but its settings could not be moved to the Trash: {e}",
                                        app.name
                                    ),
                                    items,
                                )
                            }
                        }
                    }
                }
                Outcome::Removed { items, freed }
            }
            Err(UninstallError::NeedsAdmin) => Outcome::NeedsAdmin,
            Err(e) => failed(&e.to_string(), Vec::new()),
        },
        _ => Outcome::NeedsAdmin,
    }
}

fn failed(reason: &str, removed: Vec<String>) -> Outcome {
    Outcome::Failed {
        reason: reason.to_string(),
        removed,
    }
}

/// Every listed file goes to the Trash. All paths are checked first, so a bad one stops the whole
/// thing before anything moves.
fn move_to_trash(plan: &RemovalPlan, env: &Env) -> Outcome {
    for item in &plan.items {
        let path = Path::new(&item.label);
        if !path.is_absolute() || !path.starts_with(env.home) || path == env.home {
            return failed(
                &format!(
                    "{} is not inside your home folder, so it is left alone.",
                    item.label
                ),
                Vec::new(),
            );
        }
    }
    let mut removed = Vec::new();
    let mut freed = 0;
    for item in &plan.items {
        match trash::move_to_trash(Path::new(&item.label), env.trash_dir, env.home, env.now) {
            Ok(_) => {
                freed += item.bytes.unwrap_or(0);
                removed.push(item.label.clone());
            }
            Err(e) => return failed(&e.to_string(), removed),
        }
    }
    Outcome::Removed {
        items: removed,
        freed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appimage::AppImageApps;
    use crate::flatpak::FlatpakApps;
    use crate::pacman::PacmanApps;
    use crate::runner::Output;
    use crate::source::AppSource;
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;

    struct Fake {
        calls: RefCell<Vec<Vec<String>>>,
        ok: bool,
    }

    impl CommandRunner for Fake {
        fn run(&self, _program: &str, args: &[&str]) -> Result<Output, String> {
            self.calls
                .borrow_mut()
                .push(args.iter().map(|a| a.to_string()).collect());
            Ok(Output {
                success: self.ok,
                stdout: String::new(),
                stderr: if self.ok {
                    String::new()
                } else {
                    "error: busy\n".into()
                },
            })
        }
    }

    fn fake(ok: bool) -> Fake {
        Fake {
            calls: RefCell::new(vec![]),
            ok,
        }
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![1u8; bytes]).unwrap();
    }

    struct World {
        home: tempfile::TempDir,
        trash: PathBuf,
        image: PathBuf,
        desktop: PathBuf,
        icon: PathBuf,
        src: AppImageApps,
    }

    fn world() -> World {
        let home = tempfile::tempdir().unwrap();
        let apps = home.path().join(".local/share/applications");
        let icons = home.path().join(".local/share/icons");
        let image = home.path().join("Applications/Krita.AppImage");
        write(&image, 100);
        let desktop = apps.join("krita.desktop");
        write(&desktop, 0);
        fs::write(
            &desktop,
            format!(
                "[Desktop Entry]\nType=Application\nName=Krita\nExec=\"{}\" %U\nIcon=krita\n",
                image.display()
            ),
        )
        .unwrap();
        let icon = icons.join("hicolor/krita.png");
        write(&icon, 10);
        let src = AppImageApps::in_dirs(vec![home.path().join("Applications")])
            .with_menu(apps, vec![icons]);
        let trash = home.path().join(".local/share/Trash");
        World {
            home,
            trash,
            image,
            desktop,
            icon,
            src,
        }
    }

    fn env<'a>(w: &'a World, runner: &'a dyn CommandRunner) -> Env<'a> {
        Env {
            trash_dir: &w.trash,
            home: w.home.path(),
            now: SystemTime::UNIX_EPOCH,
            runner,
            delete_settings: false,
        }
    }

    fn user_flatpak(w: &World) -> AppEntry {
        let fl = FlatpakApps::from_list(
            &fs::read_to_string(format!(
                "{}/../../tests/fixtures/flatpak/list-handwritten.txt",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        );
        let _ = w;
        fl.list().unwrap().remove(1)
    }

    /// `~/.var/app/org.mozilla.firefox/config/prefs.json` (500 bytes).
    fn var_app(w: &World) -> PathBuf {
        let dir = w.home.path().join(".var/app/org.mozilla.firefox");
        write(&dir.join("config/prefs.json"), 500);
        dir
    }

    #[test]
    fn an_appimage_its_launcher_and_its_icon_go_to_the_trash() {
        let w = world();
        let app = w.src.list().unwrap().remove(0);
        let plan = w.src.removal_plan(&app).unwrap();
        let fresh = w.src.removal_plan(&app).unwrap();
        let runner = fake(true);
        let total: u64 = plan.items.iter().filter_map(|i| i.bytes).sum();
        let out = run(&app, &plan, &fresh, &env(&w, &runner));
        match out {
            Outcome::Removed { items, freed } => {
                assert_eq!(items.len(), 3);
                assert_eq!(freed, total);
                assert!(total >= 110, "image 100 + icon 10 + launcher");
            }
            other => panic!("{other:?}"),
        }
        for gone in [&w.image, &w.desktop, &w.icon] {
            assert!(!gone.exists(), "{gone:?}");
        }
        assert!(w.trash.join("files/Krita.AppImage").exists());
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn a_list_that_changed_since_the_confirm_screen_changes_nothing() {
        let w = world();
        let app = w.src.list().unwrap().remove(0);
        let confirmed = w.src.removal_plan(&app).unwrap();
        fs::remove_file(&w.desktop).unwrap();
        let fresh = w.src.removal_plan(&app).unwrap();
        let out = run(&app, &confirmed, &fresh, &env(&w, &fake(true)));
        assert!(
            matches!(&out, Outcome::Failed { reason, removed } if reason.contains("changed") && removed.is_empty()),
            "{out:?}"
        );
        assert!(w.image.exists() && w.icon.exists());
    }

    #[test]
    fn paths_outside_the_home_folder_are_refused_before_anything_moves() {
        let w = world();
        let outside = tempfile::tempdir().unwrap();
        write(&outside.path().join("x.AppImage"), 5);
        let app = w.src.list().unwrap().remove(0);
        let mut plan = w.src.removal_plan(&app).unwrap();
        plan.items.push(crate::source::PlanItem {
            label: outside.path().join("x.AppImage").display().to_string(),
            bytes: Some(5),
        });
        let out = run(&app, &plan, &plan.clone(), &env(&w, &fake(true)));
        assert!(
            matches!(&out, Outcome::Failed { removed, .. } if removed.is_empty()),
            "{out:?}"
        );
        assert!(w.image.exists() && outside.path().join("x.AppImage").exists());
    }

    #[test]
    fn a_blocked_plan_is_never_run() {
        let w = world();
        let app = w.src.list().unwrap().remove(0);
        let plan = w.src.removal_plan(&app).unwrap();
        let mut fresh = plan.clone();
        fresh.blocked = Some(Blocked::Busy);
        let out = run(&app, &plan, &fresh, &env(&w, &fake(true)));
        assert_eq!(out, Outcome::Blocked(Blocked::Busy));
        assert!(w.image.exists());
    }

    #[test]
    fn a_failure_half_way_reports_what_already_went() {
        let w = world();
        let app = w.src.list().unwrap().remove(0);
        let plan = w.src.removal_plan(&app).unwrap();
        let fresh = plan.clone();
        // The icon vanishes between the check and the move.
        fs::remove_file(&w.icon).unwrap();
        let out = run(&app, &plan, &fresh, &env(&w, &fake(true)));
        match out {
            Outcome::Failed { removed, .. } => assert_eq!(removed.len(), 2),
            other => panic!("{other:?}"),
        }
        assert!(!w.image.exists() && w.trash.join("files/Krita.AppImage").exists());
    }

    #[test]
    fn packages_and_system_flatpaks_need_admin_and_never_reach_the_runner() {
        let w = world();
        let pac = PacmanApps::from_qi(
            &fs::read_to_string(format!(
                "{}/../../tests/fixtures/pacman/Qi-vlc.txt",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        );
        let app = pac.list().unwrap().remove(0);
        let plan = pac.removal_plan(&app).unwrap();
        let runner = fake(true);
        assert_eq!(
            run(&app, &plan, &plan.clone(), &env(&w, &runner)),
            Outcome::NeedsAdmin
        );
        let fl = FlatpakApps::from_list(
            &fs::read_to_string(format!(
                "{}/../../tests/fixtures/flatpak/list-handwritten.txt",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        );
        let sys = fl.list().unwrap().remove(0);
        let plan = fl.removal_plan(&sys).unwrap();
        assert_eq!(
            run(&sys, &plan, &plan.clone(), &env(&w, &runner)),
            Outcome::NeedsAdmin
        );
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn the_settings_folder_is_known_only_for_a_user_flatpaks_var_app_folder() {
        let w = world();
        let app = user_flatpak(&w);
        assert_eq!(settings_folder(&app, w.home.path()), None, "no folder yet");
        let dir = var_app(&w);
        let found = settings_folder(&app, w.home.path()).unwrap();
        assert_eq!(found.path, dir);
        assert!(found.bytes >= 500, "{}", found.bytes);
        // A system Flatpak, an AppImage and a package: never guessed, even with a folder there.
        let fl = FlatpakApps::from_list(
            &fs::read_to_string(format!(
                "{}/../../tests/fixtures/flatpak/list-handwritten.txt",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        );
        let sys = fl.list().unwrap().remove(0);
        write(&w.home.path().join(format!(".var/app/{}/x", sys.id)), 10);
        assert_eq!(settings_folder(&sys, w.home.path()), None);
        let image = w.src.list().unwrap().remove(0);
        write(&w.home.path().join(".config/Krita/kritarc"), 10);
        assert_eq!(settings_folder(&image, w.home.path()), None);
    }

    #[test]
    fn a_linked_or_oddly_named_settings_folder_is_not_offered() {
        let w = world();
        let app = user_flatpak(&w);
        let elsewhere = w.home.path().join("Documents");
        write(&elsewhere.join("thesis.odt"), 10);
        fs::create_dir_all(w.home.path().join(".var/app")).unwrap();
        std::os::unix::fs::symlink(
            &elsewhere,
            w.home.path().join(".var/app/org.mozilla.firefox"),
        )
        .unwrap();
        assert_eq!(settings_folder(&app, w.home.path()), None, "a link");
        for bad in ["../Documents", "a/b", "", "--force", ".."] {
            let odd = AppEntry {
                id: bad.into(),
                ..app.clone()
            };
            assert_eq!(settings_folder(&odd, w.home.path()), None, "{bad:?}");
        }
    }

    #[test]
    fn asking_for_it_moves_the_settings_to_the_trash_after_the_app_is_gone() {
        let w = world();
        let app = user_flatpak(&w);
        let dir = var_app(&w);
        let plan = FlatpakApps::from_list(
            &fs::read_to_string(format!(
                "{}/../../tests/fixtures/flatpak/list-handwritten.txt",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .removal_plan(&app)
        .unwrap();
        let runner = fake(true);
        let mut e = env(&w, &runner);
        e.delete_settings = true;
        let size = settings_folder(&app, w.home.path()).unwrap().bytes;
        let out = run(&app, &plan, &plan.clone(), &e);
        assert_eq!(
            out,
            Outcome::Removed {
                items: vec!["org.mozilla.firefox".into(), dir.display().to_string()],
                freed: 310_500_000 + size
            }
        );
        assert!(!dir.exists());
        assert!(w.trash.join("files/org.mozilla.firefox").exists());
        assert_eq!(runner.calls.borrow().len(), 1);
    }

    #[test]
    fn by_default_the_settings_stay_and_a_failed_uninstall_never_touches_them() {
        let w = world();
        let app = user_flatpak(&w);
        let dir = var_app(&w);
        let plan = RemovalPlan {
            id: app.id.clone(),
            kind: SourceKind::Flatpak,
            items: vec![],
            blocked: None,
            needs_admin: false,
            to_trash: false,
            command: vec![],
        };
        let runner = fake(true);
        run(&app, &plan, &plan.clone(), &env(&w, &runner));
        assert!(dir.join("config/prefs.json").exists(), "kept by default");
        let bad = fake(false);
        let mut e = env(&w, &bad);
        e.delete_settings = true;
        let out = run(&app, &plan, &plan.clone(), &e);
        assert!(matches!(out, Outcome::Failed { .. }), "{out:?}");
        assert!(dir.join("config/prefs.json").exists(), "uninstall failed");
    }

    #[test]
    fn asking_for_it_never_touches_other_kinds_of_apps() {
        let w = world();
        write(&w.home.path().join(".config/Krita/kritarc"), 10);
        let app = w.src.list().unwrap().remove(0);
        let plan = w.src.removal_plan(&app).unwrap();
        let runner = fake(true);
        let mut e = env(&w, &runner);
        e.delete_settings = true;
        let out = run(&app, &plan, &plan.clone(), &e);
        assert!(
            matches!(&out, Outcome::Removed { items, .. } if items.len() == 3),
            "{out:?}"
        );
        assert!(w.home.path().join(".config/Krita/kritarc").exists());
    }

    #[test]
    fn if_the_settings_cannot_be_trashed_the_app_is_still_reported_as_removed_first() {
        let w = world();
        let app = user_flatpak(&w);
        let dir = var_app(&w);
        let plan = FlatpakApps::from_list(
            &fs::read_to_string(format!(
                "{}/../../tests/fixtures/flatpak/list-handwritten.txt",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .removal_plan(&app)
        .unwrap();
        // The Trash "folder" is a file, so nothing can be moved into it.
        let not_a_folder = w.home.path().join("not-a-folder");
        write(&not_a_folder, 1);
        let runner = fake(true);
        let mut e = env(&w, &runner);
        e.trash_dir = &not_a_folder;
        e.delete_settings = true;
        let out = run(&app, &plan, &plan.clone(), &e);
        assert!(
            matches!(&out, Outcome::Failed { removed, .. } if removed == &vec!["org.mozilla.firefox".to_string()]),
            "{out:?}"
        );
        assert!(dir.join("config/prefs.json").exists(), "left where it was");
    }

    #[test]
    fn a_user_flatpak_is_removed_through_the_runner() {
        let w = world();
        let fl = FlatpakApps::from_list(
            &fs::read_to_string(format!(
                "{}/../../tests/fixtures/flatpak/list-handwritten.txt",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        );
        let app = fl.list().unwrap().remove(1);
        let plan = fl.removal_plan(&app).unwrap();
        let runner = fake(true);
        let out = run(&app, &plan, &plan.clone(), &env(&w, &runner));
        assert_eq!(
            out,
            Outcome::Removed {
                items: vec!["org.mozilla.firefox".into()],
                freed: 310_500_000
            }
        );
        assert_eq!(runner.calls.borrow().len(), 1);
        let bad = fake(false);
        let out = run(&app, &plan, &plan.clone(), &env(&w, &bad));
        assert!(
            matches!(&out, Outcome::Failed { reason, removed } if reason.contains("busy") && removed.is_empty()),
            "{out:?}"
        );
    }
}
