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
use crate::source::{AppEntry, Blocked, RemovalPlan, Scope, SourceKind};

pub struct Env<'a> {
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
            Ok(()) => Outcome::Removed {
                items: vec![app.id.clone()],
                freed: app.size_bytes.unwrap_or(0),
            },
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
        }
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
