//! Live History: reads the records when the page is opened, shows the picked entry, puts back what
//! the Trash still holds (Undo) and copies a plain-text report for a help forum.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;
use tidewise_core::history::{self, Record};

use crate::{history_view, records, view, HistState, MainWindow, ToastState};

#[derive(Default)]
pub struct Ctl {
    records: Vec<Record>,
    selected: Option<String>,
    /// `Arch Linux · KDE Plasma 6 · pacman + Flatpak`, for the forum report.
    system: String,
    offset: i64,
    toast_timer: slint::Timer,
}

/// Seconds east of UTC, from `date +%z` (`+0500`); 0 (UTC) when it cannot be read.
fn local_offset() -> i64 {
    let Ok(out) = std::process::Command::new("/usr/bin/date")
        .arg("+%z")
        .env("LC_ALL", "C")
        .output()
    else {
        return 0;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim();
    let (sign, digits) = match text.split_at_checked(1) {
        Some(("+", d)) => (1, d),
        Some(("-", d)) => (-1, d),
        _ => return 0,
    };
    match (digits.get(..2), digits.get(2..4)) {
        (Some(h), Some(m)) => match (h.parse::<i64>(), m.parse::<i64>()) {
            (Ok(h), Ok(m)) => sign * (h * 3600 + m * 60),
            _ => 0,
        },
        _ => 0,
    }
}

impl Ctl {
    pub fn new(system: String) -> Self {
        Ctl {
            system,
            offset: local_offset(),
            ..Ctl::default()
        }
    }

    /// The page was opened: read the records again.
    pub fn enter(&mut self, ui: &MainWindow) {
        self.records = history::history_dir()
            .map(|dir| history::list(&dir))
            .unwrap_or_default();
        self.refresh(ui);
    }

    fn refresh(&mut self, ui: &MainWindow) {
        let screen = history_view::screen(
            self.records.clone(),
            self.selected.as_deref(),
            records::now_secs(),
            self.offset,
        );
        self.selected = screen.detail.as_ref().map(|d| d.id.clone());
        let empty = if screen.days.is_empty() {
            Some((
                1,
                "Cleaning up, uninstalling, moving files to the Trash and ending apps will be listed here.",
            ))
        } else {
            None
        };
        view::apply_history(ui, &screen, empty);
    }

    fn picked(&self) -> Option<&Record> {
        let id = self.selected.as_deref()?;
        self.records.iter().find(|r| r.id == id)
    }

    fn toast(&mut self, ui: &MainWindow, message: String, error: bool) {
        let t = ui.global::<ToastState>();
        t.set_message(message.into());
        t.set_can_undo(false);
        t.set_error(error);
        t.set_shown(true);
        let weak = ui.as_weak();
        self.toast_timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_secs(8),
            move || {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<ToastState>().set_shown(false);
                }
            },
        );
    }

    fn undo(&mut self, ui: &MainWindow) {
        let Some(record) = self.picked().cloned() else {
            return;
        };
        let Some(trash) = tidewise_core::trash::default_trash_dir() else {
            self.toast(
                ui,
                "Couldn't find your Trash folder. Nothing was moved.".into(),
                true,
            );
            return;
        };
        let report = history::undo_trash(&record, &trash);
        let (message, error) = match report.failed.first() {
            None => (
                format!(
                    "Put back {} file{}",
                    report.restored,
                    if report.restored == 1 { "" } else { "s" }
                ),
                false,
            ),
            Some((name, why)) => (format!("Couldn't put {name} back: {why}"), true),
        };
        self.toast(ui, message, error);
    }

    fn copy(&mut self, ui: &MainWindow) {
        let Some(record) = self.picked().cloned() else {
            return;
        };
        let text = history::report_text(&record, &self.system);
        match crate::clipboard::copy(&text) {
            Ok(_) => self.toast(
                ui,
                "Copied. You can paste it in your help request.".into(),
                false,
            ),
            Err(_) => {
                // No clipboard program: keep the report in a file next to the record instead.
                let saved = history::history_dir().and_then(|dir| {
                    let path = dir.join(format!("{}-report.txt", record.id));
                    std::fs::write(&path, &text).ok().map(|()| path)
                });
                match saved {
                    Some(path) => self.toast(
                        ui,
                        format!(
                            "No clipboard program found. The report is saved in {}",
                            path.display()
                        ),
                        false,
                    ),
                    None => self.toast(ui, "The report could not be copied or saved.".into(), true),
                }
            }
        }
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    let st = ui.global::<HistState>();
    st.on_pick({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |id| {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.selected = Some(id.to_string());
            c.refresh(&ui);
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
    st.on_copy({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let (Some(ui), mut c) = (weak.upgrade(), ctl.borrow_mut()) else {
                return;
            };
            c.copy(&ui);
        }
    });
}
