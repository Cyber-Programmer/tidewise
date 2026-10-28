//! Live Storage screen: starts scans in the background, polls them once a second and keeps
//! what the user ticked. The view data itself comes from `storage::live_data`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use slint::ComponentHandle;
use tidewise_core::drive::{drives, Drive};
use tidewise_core::trash::{self, Trashed};

use crate::nav::{self, Browser, Place};
use crate::storage::{self, FolderIn, Phase};
use crate::{MainWindow, StorageState, ToastState};

/// A breadcrumb longer than this many characters collapses its middle into `…`.
const CRUMB_CHARS: usize = 64;

pub struct Ctl {
    drives: Vec<Drive>,
    picked: usize,
    home: PathBuf,
    /// Screenshots of a fixture: scan exactly this folder, whatever drive is picked.
    fixed_root: Option<PathBuf>,
    browser: Browser,
    started: bool,
    /// File of each row of Biggest files, in order.
    big: Vec<Option<PathBuf>>,
    /// What the last Trash action moved, for Undo.
    trashed: Vec<Trashed>,
    toast_timer: slint::Timer,
}

/// The folder a drive is scanned from: the home folder if it lives on the drive, else the drive.
pub fn scan_root(drives: &[Drive], picked: usize, home: &Path) -> PathBuf {
    let holds_home = drives
        .iter()
        .filter(|d| home.starts_with(&d.mount_point))
        .max_by_key(|d| d.mount_point.len());
    match (holds_home, drives.get(picked)) {
        (Some(h), Some(p)) if h.mount_point == p.mount_point => home.to_path_buf(),
        (_, Some(p)) => PathBuf::from(&p.mount_point),
        _ => home.to_path_buf(),
    }
}

impl Ctl {
    pub fn new() -> Self {
        Self::new_inner(
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default(),
        )
    }

    /// A controller that treats `home` as the user's files (screenshots of a fixture folder).
    pub fn with_home(home: PathBuf) -> Self {
        let mut ctl = Self::new_inner(home.clone());
        ctl.fixed_root = Some(home);
        ctl
    }

    fn new_inner(home: PathBuf) -> Self {
        Ctl {
            drives: Vec::new(),
            picked: 0,
            browser: Browser::new(home.clone()),
            home,
            fixed_root: None,
            started: false,
            big: Vec::new(),
            trashed: Vec::new(),
            toast_timer: slint::Timer::default(),
        }
    }

    fn root_dir(&self) -> PathBuf {
        self.fixed_root
            .clone()
            .unwrap_or_else(|| scan_root(&self.drives, self.picked, &self.home))
    }

    /// Called when the page opens.
    pub fn enter(&mut self, ui: &MainWindow) {
        if self.drives.is_empty() {
            self.drives = drives();
        }
        if !self.started {
            self.started = true;
            self.browser.set_root(self.root_dir());
        }
        self.refresh(ui);
    }

    /// No scan is running.
    pub fn is_idle(&self) -> bool {
        self.browser.is_idle()
    }

    /// Show a folder below the scanned one (`--open`).
    pub fn open_now(&mut self, rel: &Path, ui: &MainWindow) -> bool {
        let ok = self.browser.open_path(rel);
        self.refresh(ui);
        ok
    }

    /// Called once a second while the page is open.
    pub fn poll(&mut self, ui: &MainWindow) {
        if self.browser.poll() {
            self.refresh(ui);
        }
    }

    fn crumbs(&self, drive_name: &str) -> Vec<nav::Crumb> {
        let label = if self.browser.root() == self.home {
            "Your files"
        } else {
            "Files"
        };
        let mut names: Vec<String> = self
            .browser
            .trail()
            .places()
            .iter()
            .map(|p| match p {
                Place::Other => "Other".to_string(),
                Place::Folder(f) => storage::plain_name(f),
            })
            .collect();
        if let (Some(last), Some(level)) = (names.last_mut(), self.browser.level()) {
            *last = format!("{last} \u{b7} {}", storage::fmt_size(level.total));
        }
        nav::collapse(nav::crumbs(drive_name, label, &names), CRUMB_CHARS)
    }

    fn refresh(&mut self, ui: &MainWindow) {
        let scanning = match (self.browser.measuring_root(), self.browser.level()) {
            (true, _) => true,
            (false, None) => !self.browser.trail().is_empty() && !self.browser.is_idle(),
            _ => false,
        };
        let empty = tidewise_core::scan::ScanResult::default();
        let phase = match (scanning, self.browser.result()) {
            (true, _) => Phase::Scanning {
                bytes: self.browser.progress_bytes().unwrap_or(0),
            },
            (false, Some(r)) => Phase::Done(r),
            (false, None) => Phase::Done(&empty),
        };
        let drive_name = self
            .drives
            .get(self.picked)
            .map(storage::drive_title)
            .unwrap_or_default();
        let folder = match (self.browser.level(), self.browser.trail().is_empty()) {
            (Some(level), false) => Some(FolderIn {
                crumbs: self.crumbs(&drive_name),
                entries: &level.items,
                checked: self.browser.checked(),
                cursor: self.browser.cursor(),
                rest: self.browser.level().and_then(|l| l.rest),
                note: self.browser.level().and_then(nav::overlap_note),
            }),
            _ => None,
        };
        let Some(data) = storage::live_data(
            &self.drives,
            self.picked,
            phase,
            folder,
            &self.home,
            SystemTime::now(),
        ) else {
            return;
        };
        self.big = data.big.iter().map(|b| b.path.clone()).collect();
        crate::view::apply_storage(ui, &data);
    }

    fn toast(&mut self, ui: &MainWindow, message: String, can_undo: bool, error: bool) {
        let t = ui.global::<ToastState>();
        t.set_message(message.into());
        t.set_can_undo(can_undo);
        t.set_error(error);
        t.set_shown(true);
        let weak = ui.as_weak();
        self.toast_timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_secs(8),
            move || {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<ToastState>().set_shown(false);
                }
            },
        );
    }

    /// Move these paths to the Trash, tell the user, and remember them for Undo.
    fn trash_paths(&mut self, paths: Vec<PathBuf>, ui: &MainWindow) {
        let Some(dir) = trash::default_trash_dir() else {
            self.toast(
                ui,
                "Couldn't find your Trash folder. Nothing was moved.".into(),
                false,
                true,
            );
            return;
        };
        let (mut moved, mut failed) = (Vec::new(), Vec::new());
        for path in paths {
            match trash::move_to_trash(&path, &dir, &self.home, SystemTime::now()) {
                Ok(t) => moved.push(t),
                Err(e) => failed.push((
                    path.file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    why(&e),
                )),
            }
        }
        let message = storage::toast_text(moved.len(), &failed);
        let any = !moved.is_empty();
        let originals: Vec<PathBuf> = moved.iter().map(|t| t.original.clone()).collect();
        self.trashed = moved;
        self.toast(ui, message, any, !failed.is_empty());
        if any {
            self.browser.forget(&originals);
        }
        self.refresh(ui);
    }

    /// Put back what the last Trash action moved.
    fn undo(&mut self, ui: &MainWindow) {
        let items = std::mem::take(&mut self.trashed);
        let mut failed = Vec::new();
        for t in items.iter().rev() {
            if let Err(e) = trash::restore(t) {
                failed.push((
                    t.original
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    why(&e),
                ));
            }
        }
        let message = match failed.first() {
            None => format!(
                "Put back {} file{}",
                items.len(),
                if items.len() == 1 { "" } else { "s" }
            ),
            Some((name, reason)) => format!("Couldn't put {name} back: {reason}"),
        };
        self.toast(ui, message, false, !failed.is_empty());
        let originals: Vec<PathBuf> = items.iter().map(|t| t.original.clone()).collect();
        self.browser.forget(&originals);
        self.refresh(ui);
    }
}

/// A short reason a person can read.
fn why(e: &trash::Error) -> String {
    match e {
        trash::Error::OtherDrive(_) => "It is on a different drive than your Trash.".into(),
        trash::Error::Protected(_) => "It is protected.".into(),
        trash::Error::NotFound(_) => "It is already gone.".into(),
        trash::Error::NotAbsolute(_) => "Its location is not clear.".into(),
        trash::Error::Occupied(_) => "Something else is already in its place.".into(),
        trash::Error::Io { source, .. } => match source.kind() {
            std::io::ErrorKind::PermissionDenied => "You don't have permission.".into(),
            _ => "The system refused.".into(),
        },
    }
}

pub fn wire(ui: &MainWindow, ctl: &Rc<RefCell<Ctl>>) {
    let st = ui.global::<StorageState>();
    st.on_pick_drive({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            if (i as usize) < c.drives.len() {
                c.picked = i as usize;
                let root = c.root_dir();
                c.browser.set_root(root);
                c.refresh(&ui);
            }
        }
    });
    st.on_open_block({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            c.browser.open_block(i.max(0) as usize);
            c.refresh(&ui);
        }
    });
    st.on_open_hint({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            let block = c.browser.result().and_then(|r| {
                storage::blocks_from(r, nav::MAP_BLOCKS)
                    .iter()
                    .position(|b| b.name == "Downloads")
            });
            if let Some(i) = block {
                c.browser.open_block(i);
            }
            c.refresh(&ui);
        }
    });
    st.on_jump({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |n| {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            c.browser.jump(n.max(0) as usize);
            c.refresh(&ui);
        }
    });
    st.on_open_row({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            c.browser.open_row(i.max(0) as usize);
            c.refresh(&ui);
        }
    });
    st.on_toggle({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            c.browser.toggle(i.max(0) as usize);
            c.refresh(&ui);
        }
    });
    st.on_key({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |text, alt| {
            let (Some(ui), Some(action)) = (weak.upgrade(), nav::key_action(&text, alt)) else {
                return false;
            };
            let mut c = ctl.borrow_mut();
            c.browser.key(action);
            c.refresh(&ui);
            true
        }
    });
    st.on_trash_selected({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            let paths: Vec<PathBuf> = c
                .browser
                .level()
                .map(|l| {
                    l.items
                        .iter()
                        .zip(c.browser.checked())
                        .filter(|(_, on)| **on)
                        .map(|(e, _)| e.path.clone())
                        .collect()
                })
                .unwrap_or_default();
            c.trash_paths(paths, &ui);
        }
    });
    st.on_trash_one({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            if let Some(Some(path)) = c.big.get(i as usize).cloned() {
                c.trash_paths(vec![path], &ui);
            }
        }
    });
    ui.global::<ToastState>().on_undo({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            ctl.borrow_mut().undo(&ui);
        }
    });
    st.on_cancel_scan({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            c.browser.cancel();
            if !c.browser.trail().is_empty() {
                c.browser.up();
            }
            c.refresh(&ui);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(mount: &str) -> Drive {
        Drive {
            name: "/dev/x".into(),
            mount_point: mount.into(),
            fs_type: "ext4".into(),
            total: 10,
            used: 5,
            free: 5,
        }
    }

    #[test]
    fn every_trash_error_has_a_plain_reason() {
        let p = PathBuf::from("/x");
        for e in [
            trash::Error::OtherDrive(p.clone()),
            trash::Error::Protected(p.clone()),
            trash::Error::NotFound(p.clone()),
            trash::Error::NotAbsolute(p.clone()),
            trash::Error::Occupied(p.clone()),
        ] {
            let reason = why(&e);
            assert!(reason.ends_with('.') && !reason.contains('/'), "{reason}");
        }
    }

    #[test]
    fn the_drive_with_home_scans_home_others_scan_their_mount() {
        let drives = [drive("/"), drive("/home"), drive("/mnt/data")];
        let home = Path::new("/home/ali");
        assert_eq!(scan_root(&drives, 1, home), home);
        assert_eq!(scan_root(&drives, 0, home), Path::new("/"));
        assert_eq!(scan_root(&drives, 2, home), Path::new("/mnt/data"));
        assert_eq!(scan_root(&[drive("/")], 0, home), home);
    }
}
