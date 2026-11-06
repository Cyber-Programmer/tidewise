//! Live Startup apps: reads the autostart folders when the page is opened, flips a switch through
//! `tidewise_adapters::autostart`, writes a History record and offers Undo in the page.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use slint::ComponentHandle;
use tidewise_adapters::autostart::{self, Dirs, StartupEntry};
use tidewise_core::history::{self, Kind};

use crate::startup::{self, Paths};
use crate::{records, view, MainWindow, StartupState};

#[derive(Default)]
pub struct Ctl {
    entries: Vec<StartupEntry>,
    /// The last switch that was flipped: its file and name, and whether it is now on.
    last: Option<(String, String, bool)>,
    toast: String,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn paths() -> Option<Paths> {
    let var = |k: &str| std::env::var(k).ok();
    Some(startup::paths_from(
        var("XDG_CONFIG_DIRS").as_deref(),
        var("XDG_CONFIG_HOME").as_deref(),
        var("XDG_STATE_HOME").as_deref(),
        &home()?,
    ))
}

impl Ctl {
    pub fn new() -> Self {
        Ctl::default()
    }

    /// The page was opened: read the folders again.
    pub fn enter(&mut self, ui: &MainWindow) {
        self.last = None;
        self.toast.clear();
        self.reload(ui);
    }

    fn reload(&mut self, ui: &MainWindow) {
        let Some(p) = paths() else {
            view::apply_startup(
                ui,
                &startup::Screen::default(),
                Some((3, "Tidewise could not find your home folder.")),
                "",
            );
            return;
        };
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
        self.entries = autostart::list(&p.system, &p.user, desktop.as_deref());
        let screen = startup::screen(&self.entries);
        let empty = self.entries.is_empty().then_some((
            2,
            "Nothing in your startup folders. Apps that start on their own would be listed here.",
        ));
        view::apply_startup(ui, &screen, empty, &self.toast);
    }

    /// Turn `file` on or off. Returns the plain sentence to show.
    fn set(&mut self, file: &str, name: &str, on: bool) -> Result<(), String> {
        let (p, home, trash) = (
            paths().ok_or("no home folder")?,
            home().ok_or("no home folder")?,
            tidewise_core::trash::default_trash_dir().ok_or("no Trash folder")?,
        );
        let dirs = Dirs {
            system: &p.system,
            user: &p.user,
            backup: &p.backup,
            trash: &trash,
            home: &home,
            now: SystemTime::now(),
        };
        let result = if on {
            autostart::enable(file, &dirs)
        } else {
            autostart::disable(file, &dirs)
        };
        result.map_err(|e| e.to_string())?;
        let written = p.user.join(file).display().to_string();
        let _ = records::save(Kind::Startup, |time, id| {
            history::startup_record(name, on, &written, time, id)
        });
        Ok(())
    }

    fn toggle(&mut self, ui: &MainWindow, file: &str) {
        let Some(entry) = self.entries.iter().find(|e| e.file == file).cloned() else {
            return;
        };
        let now_on = !entry.enabled;
        match self.set(file, &entry.name, now_on) {
            Ok(()) => {
                self.toast = startup::toast_text(&entry.name, now_on);
                self.last = Some((file.to_string(), entry.name, now_on));
            }
            Err(why) => {
                self.toast = startup::failure_text(&entry.name, &why);
                self.last = None;
            }
        }
        self.reload(ui);
    }

    fn undo(&mut self, ui: &MainWindow) {
        let Some((file, name, now_on)) = self.last.take() else {
            return;
        };
        self.toast = match self.set(&file, &name, !now_on) {
            Ok(()) => String::new(),
            Err(why) => startup::failure_text(&name, &why),
        };
        self.reload(ui);
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    let st = ui.global::<StartupState>();
    st.on_toggle({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |file| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.toggle(&ui, &file);
        }
    });
    st.on_undo({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.undo(&ui);
        }
    });
}
