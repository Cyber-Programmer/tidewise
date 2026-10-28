//! Live Storage screen: starts scans in the background, polls them once a second and keeps
//! what the user ticked. The view data itself comes from `storage::live_data`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::SystemTime;

use slint::ComponentHandle;
use tidewise_core::drive::{drives, Drive};
use tidewise_core::mounts::{home_access_time, TimeSource};
use tidewise_core::scan::{self, ScanHandle, ScanOptions, ScanResult};
use tidewise_core::trash::{self, Trashed};

use crate::storage::{self, Entry, Phase};
use crate::{MainWindow, StorageState, ToastState};

/// How many rows the drill-in list shows.
const FOLDER_ROWS: usize = 12;

enum Job {
    Root(ScanHandle),
    Folder(ScanHandle, PathBuf),
}

struct Opened {
    path: PathBuf,
    title: String,
    entries: Vec<Entry>,
    checked: Vec<bool>,
}

pub struct Ctl {
    drives: Vec<Drive>,
    picked: usize,
    home: PathBuf,
    job: Option<Job>,
    root: Option<ScanResult>,
    opened: Option<Opened>,
    /// Folder of each block on screen, in order.
    tiles: Vec<Option<PathBuf>>,
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
        Ctl {
            drives: Vec::new(),
            picked: 0,
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default(),
            job: None,
            root: None,
            opened: None,
            tiles: Vec::new(),
            big: Vec::new(),
            trashed: Vec::new(),
            toast_timer: slint::Timer::default(),
        }
    }

    fn options(biggest: usize) -> ScanOptions {
        ScanOptions {
            biggest,
            use_access_time: home_access_time().source == TimeSource::AccessTime,
            ..ScanOptions::default()
        }
    }

    fn start_root(&mut self) {
        let root = scan_root(&self.drives, self.picked, &self.home);
        self.root = None;
        self.opened = None;
        self.job = Some(Job::Root(scan::start(root, Self::options(500))));
    }

    /// Called when the page opens.
    pub fn enter(&mut self, ui: &MainWindow) {
        if self.drives.is_empty() {
            self.drives = drives();
        }
        if self.root.is_none() && self.job.is_none() {
            self.start_root();
        }
        self.refresh(ui);
    }

    /// Called once a second while the page is open.
    pub fn poll(&mut self, ui: &MainWindow) {
        let finished = match &self.job {
            Some(Job::Root(h) | Job::Folder(h, _)) => h.is_finished(),
            None => false,
        };
        if finished {
            match self.job.take() {
                Some(Job::Root(h)) => self.root = Some(h.wait()),
                Some(Job::Folder(h, path)) => self.finish_folder(&path, h.wait()),
                None => {}
            }
        }
        if finished || self.job.is_some() {
            self.refresh(ui);
        }
    }

    fn finish_folder(&mut self, path: &Path, result: ScanResult) {
        if result.cancelled {
            return;
        }
        let now = SystemTime::now();
        let by_access =
            !result.used_modified_time && home_access_time().source == TimeSource::AccessTime;
        let entries: Vec<Entry> = result
            .children
            .iter()
            .take(FOLDER_ROWS)
            .filter_map(|(p, bytes)| {
                let meta = std::fs::symlink_metadata(p).ok()?;
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let used = if by_access {
                    meta.accessed().unwrap_or(modified).max(modified)
                } else {
                    modified
                };
                Some(Entry {
                    name: p.file_name()?.to_string_lossy().into_owned(),
                    path: p.clone(),
                    bytes: *bytes,
                    is_dir: meta.is_dir(),
                    days_unused: storage::days_since(now, used),
                })
            })
            .collect();
        let checked = entries
            .iter()
            .map(|e| e.days_unused >= storage::OLD_DAYS)
            .collect();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.opened = Some(Opened {
            path: path.to_path_buf(),
            title: format!("{name} \u{b7} {}", storage::fmt_size(result.total_bytes)),
            entries,
            checked,
        });
    }

    fn refresh(&mut self, ui: &MainWindow) {
        let progress = match &self.job {
            Some(Job::Root(h) | Job::Folder(h, _)) => {
                Some(h.progress.bytes.load(Ordering::Relaxed))
            }
            None => None,
        };
        let empty = ScanResult::default();
        let phase = match (progress, &self.root) {
            (Some(bytes), _) => Phase::Scanning { bytes },
            (None, Some(r)) => Phase::Done(r),
            (None, None) => Phase::Done(&empty),
        };
        let folder = self.opened.as_ref().map(|o| {
            let entries = o
                .entries
                .iter()
                .map(|e| Entry {
                    name: e.name.clone(),
                    path: e.path.clone(),
                    bytes: e.bytes,
                    is_dir: e.is_dir,
                    days_unused: e.days_unused,
                })
                .collect();
            let parent = if self.root.as_ref().is_some_and(|r| r.root == self.home) {
                "Your files"
            } else {
                "Files"
            };
            (parent, o.title.as_str(), entries)
        });
        let Some(mut data) = storage::live_data(
            &self.drives,
            self.picked,
            phase,
            folder,
            &self.home,
            SystemTime::now(),
        ) else {
            return;
        };
        if let (Some(f), Some(o)) = (data.folder.as_mut(), self.opened.as_ref()) {
            for (row, on) in f.rows.iter_mut().zip(&o.checked) {
                row.checked = *on;
            }
        }
        self.tiles = data.tiles.iter().map(|t| t.path.clone()).collect();
        self.big = data.big.iter().map(|b| b.path.clone()).collect();
        crate::view::apply_storage(ui, &data);
    }

    /// Measure the drive again, keeping the open folder on screen's data.
    fn rescan(&mut self) {
        self.stop();
        let root = scan_root(&self.drives, self.picked, &self.home);
        self.root = None;
        self.job = Some(Job::Root(scan::start(root, Self::options(500))));
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
        if let Some(o) = self.opened.as_mut() {
            let keep: Vec<bool> = o
                .entries
                .iter()
                .map(|e| !moved.iter().any(|t| t.original == e.path))
                .collect();
            let mut k = keep.iter();
            o.entries.retain(|_| *k.next().unwrap_or(&true));
            let mut k = keep.iter();
            o.checked.retain(|_| *k.next().unwrap_or(&true));
        }
        let message = storage::toast_text(moved.len(), &failed);
        let any = !moved.is_empty();
        self.trashed = moved;
        self.toast(ui, message, any, !failed.is_empty());
        if any {
            self.rescan();
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
        let reopen = self.opened.as_ref().map(|o| o.path.clone());
        self.rescan();
        if let Some(path) = reopen {
            self.open(path, ui);
        }
        self.refresh(ui);
    }

    fn stop(&mut self) {
        if let Some(Job::Root(h) | Job::Folder(h, _)) = &self.job {
            h.cancel();
        }
        self.job = None;
    }

    fn open(&mut self, path: PathBuf, ui: &MainWindow) {
        self.stop();
        self.job = Some(Job::Folder(
            scan::start(path.clone(), Self::options(1)),
            path,
        ));
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
                c.stop();
                c.picked = i as usize;
                c.start_root();
                c.refresh(&ui);
            }
        }
    });
    st.on_open_block({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            if let Some(Some(path)) = c.tiles.get(i as usize).cloned() {
                c.open(path, &ui);
            }
        }
    });
    st.on_open_hint({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            let downloads = c.home.join("Downloads");
            c.open(downloads, &ui);
        }
    });
    st.on_go_root({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            c.opened = None;
            c.refresh(&ui);
        }
    });
    st.on_toggle({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            if let Some(on) = c
                .opened
                .as_mut()
                .and_then(|o| o.checked.get_mut(i as usize))
            {
                *on = !*on;
            }
            c.refresh(&ui);
        }
    });
    st.on_trash_selected({
        let (weak, ctl) = (ui.as_weak(), ctl.clone());
        move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut c = ctl.borrow_mut();
            let paths: Vec<PathBuf> = c
                .opened
                .as_ref()
                .map(|o| {
                    o.entries
                        .iter()
                        .zip(&o.checked)
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
            c.stop();
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
