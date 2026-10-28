//! Going as deep as you like in Storage: the path stack, breadcrumbs, scanned levels and their
//! cache, ticks and keys. Plain Rust, no Slint types, so it is tested without a window.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::SystemTime;

use tidewise_core::mounts::{home_access_time, TimeSource};
use tidewise_core::scan::{self, ScanHandle, ScanOptions, ScanResult};

use crate::storage::{self, RowKind};

/// How many blocks the folder map has (the last one may be "Other").
pub const MAP_BLOCKS: usize = 6;

/// Somewhere you can be below the map: a folder, or the "Other" block's list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    Other,
    Folder(PathBuf),
}

/// The places opened so far, outermost first. Empty means the map itself.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Trail(Vec<Place>);

impl Trail {
    pub fn push(&mut self, place: Place) {
        self.0.push(place);
    }
    pub fn pop(&mut self) {
        self.0.pop();
    }
    /// Keep the first `n` places.
    pub fn jump(&mut self, n: usize) {
        self.0.truncate(n);
    }
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn current(&self) -> Option<&Place> {
        self.0.last()
    }
    pub fn places(&self) -> &[Place] {
        &self.0
    }
}

/// One part of the breadcrumb. `target` is how many places to keep when it is clicked; the last
/// part (where you are) has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub target: Option<usize>,
}

/// `Main drive › Your files › Documents › Creative Chaos`.
pub fn crumbs(drive: &str, files: &str, trail: &[String]) -> Vec<Crumb> {
    let mut out = vec![
        Crumb {
            label: drive.into(),
            target: Some(0),
        },
        Crumb {
            label: files.into(),
            target: if trail.is_empty() { None } else { Some(0) },
        },
    ];
    for (i, name) in trail.iter().enumerate() {
        out.push(Crumb {
            label: name.clone(),
            target: if i + 1 == trail.len() {
                None
            } else {
                Some(i + 1)
            },
        });
    }
    out
}

/// Shorten a breadcrumb that is longer than `max_chars` (parts joined by ` › `): keep the first
/// part and the last two, put `…` in between.
pub fn collapse(c: Vec<Crumb>, max_chars: usize) -> Vec<Crumb> {
    let n = c.len();
    let width = c.iter().map(|c| c.label.chars().count()).sum::<usize>() + 3 * n.saturating_sub(1);
    if n <= 3 || width <= max_chars {
        return c;
    }
    vec![
        c[0].clone(),
        Crumb {
            label: "\u{2026}".into(),
            target: c[n - 3].target,
        },
        c[n - 2].clone(),
        c[n - 1].clone(),
    ]
}

/// One row of a level.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub kind: RowKind,
    pub days_unused: u64,
}

/// The most rows a level lists; the rest are counted in one last row.
pub const ROW_CAP: usize = 2000;

/// What one opened folder holds, biggest first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Level {
    pub items: Vec<Item>,
    pub total: u64,
    /// How many smaller items were left out of the list, and their bytes.
    pub rest: Option<(usize, u64)>,
}

/// The first `cap` children, and how many more there were with their bytes.
pub fn split_cap(
    children: &[(PathBuf, u64)],
    _cap: usize,
) -> (&[(PathBuf, u64)], Option<(usize, u64)>) {
    (children, None)
}

/// "and 35 smaller items · 12 MB".
pub fn rest_text(_count: usize, _bytes: u64) -> String {
    String::new()
}

/// Read what kind of thing each child is and when it was last used. Things that vanished are left out.
fn items_from(
    children: &[(PathBuf, u64)],
    other_drives: &[PathBuf],
    now: SystemTime,
    by_access: bool,
) -> Vec<Item> {
    let mut items: Vec<Item> = children
        .iter()
        .filter_map(|(p, bytes)| {
            let meta = std::fs::symlink_metadata(p).ok()?;
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let used = if by_access {
                meta.accessed().unwrap_or(modified).max(modified)
            } else {
                modified
            };
            let kind = if meta.file_type().is_symlink() {
                RowKind::Link
            } else if meta.is_dir() {
                RowKind::Folder
            } else {
                RowKind::File
            };
            Some(Item {
                name: p.file_name()?.to_string_lossy().into_owned(),
                path: p.clone(),
                bytes: *bytes,
                kind,
                days_unused: storage::days_since(now, used),
            })
        })
        .collect();
    items.extend(other_drives.iter().filter_map(|p| {
        Some(Item {
            name: p.file_name()?.to_string_lossy().into_owned(),
            path: p.clone(),
            bytes: 0,
            kind: RowKind::OtherDrive,
            days_unused: 0,
        })
    }));
    items
}

pub fn level_from(r: &ScanResult, now: SystemTime, by_access: bool, _cap: usize) -> Level {
    Level {
        items: items_from(&r.children, &r.other_drives, now, by_access),
        total: r.total_bytes,
        rest: None,
    }
}

/// The folders the map groups into "Other" (all but the biggest `max_blocks - 1`).
pub fn other_level(
    root: &ScanResult,
    max_blocks: usize,
    now: SystemTime,
    by_access: bool,
    _cap: usize,
) -> Level {
    let members = storage::other_members(root, max_blocks);
    Level {
        total: members.iter().map(|m| m.1).sum(),
        items: items_from(&members, &[], now, by_access),
        rest: None,
    }
}

/// Levels already scanned, so going back is instant.
#[derive(Default)]
pub struct Cache(HashMap<Place, Rc<Level>>);

impl Cache {
    pub fn put(&mut self, place: Place, level: Rc<Level>) {
        self.0.insert(place, level);
    }
    pub fn get(&self, place: &Place) -> Option<Rc<Level>> {
        self.0.get(place).cloned()
    }
    /// Forget every level that holds one of these paths (its own size changed).
    pub fn invalidate_for(&mut self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        self.0.retain(|place, _| match place {
            Place::Other => false,
            Place::Folder(f) => !paths.iter().any(|p| p.starts_with(f)),
        });
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Open,
    Toggle,
    Up,
    Prev,
    Next,
}

/// Enter opens, Space ticks, Backspace or Alt+Left goes up, arrows move.
pub fn key_action(text: &str, alt: bool) -> Option<KeyAction> {
    match text {
        "\n" | "\r" => Some(KeyAction::Open),
        " " => Some(KeyAction::Toggle),
        "\u{8}" => Some(KeyAction::Up),
        "\u{F702}" if alt => Some(KeyAction::Up),
        "\u{F700}" => Some(KeyAction::Prev),
        "\u{F701}" => Some(KeyAction::Next),
        _ => None,
    }
}

/// The Storage page without any window: the map's scan, where you are, what is ticked.
pub struct Browser {
    root: PathBuf,
    result: Option<ScanResult>,
    root_job: Option<ScanHandle>,
    level_job: Option<(ScanHandle, Place)>,
    trail: Trail,
    cache: Cache,
    level: Option<Rc<Level>>,
    checked: Vec<bool>,
    cursor: usize,
    scans: usize,
    access_time: bool,
}

impl Browser {
    pub fn new(root: PathBuf) -> Self {
        Browser {
            root,
            result: None,
            root_job: None,
            level_job: None,
            trail: Trail::default(),
            cache: Cache::default(),
            level: None,
            checked: Vec::new(),
            cursor: 0,
            scans: 0,
            access_time: home_access_time().source == TimeSource::AccessTime,
        }
    }

    fn options(&self, biggest: usize) -> ScanOptions {
        ScanOptions {
            biggest,
            use_access_time: self.access_time,
            ..ScanOptions::default()
        }
    }

    fn by_access(&self) -> bool {
        self.access_time && !self.result.as_ref().is_some_and(|r| r.used_modified_time)
    }

    /// The folder the map is made from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Change the folder the map is made from and measure it.
    pub fn set_root(&mut self, root: PathBuf) {
        self.root = root;
        self.start();
    }

    /// Measure the root again from nothing: the map, and every level forgotten.
    pub fn start(&mut self) {
        self.cancel();
        self.cache.clear();
        self.trail = Trail::default();
        self.level = None;
        self.checked.clear();
        self.cursor = 0;
        self.result = None;
        self.scans += 1;
        self.root_job = Some(scan::start(self.root.clone(), self.options(500)));
    }

    pub fn result(&self) -> Option<&ScanResult> {
        self.result.as_ref()
    }

    /// Bytes looked at so far by the scan that is running, if one is.
    pub fn progress_bytes(&self) -> Option<u64> {
        let h = self
            .root_job
            .as_ref()
            .or(self.level_job.as_ref().map(|(h, _)| h))?;
        Some(h.progress.bytes.load(Ordering::Relaxed))
    }

    /// True while the map itself is being measured.
    pub fn measuring_root(&self) -> bool {
        self.root_job.is_some()
    }

    /// Stop whatever is being scanned.
    pub fn cancel(&mut self) {
        if let Some(h) = &self.root_job {
            h.cancel();
        }
        if let Some((h, _)) = &self.level_job {
            h.cancel();
        }
        self.root_job = None;
        self.level_job = None;
    }

    /// Collect finished scans. True when something on screen changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = self.root_job.is_some() || self.level_job.is_some();
        if self.root_job.as_ref().is_some_and(|h| h.is_finished()) {
            if let Some(h) = self.root_job.take() {
                self.result = Some(h.wait());
            }
            changed = true;
            if !self.trail.is_empty() {
                self.load_current();
            }
        }
        if self
            .level_job
            .as_ref()
            .is_some_and(|(h, _)| h.is_finished())
        {
            if let Some((h, place)) = self.level_job.take() {
                let result = h.wait();
                if !result.cancelled && self.trail.current() == Some(&place) {
                    let level = Rc::new(level_from(
                        &result,
                        SystemTime::now(),
                        self.by_access(),
                        ROW_CAP,
                    ));
                    self.cache.put(place, level.clone());
                    self.show(level);
                }
            }
            changed = true;
        }
        changed
    }

    pub fn is_idle(&self) -> bool {
        self.root_job.is_none() && self.level_job.is_none()
    }

    #[cfg(test)]
    pub fn scans_started(&self) -> usize {
        self.scans
    }

    pub fn trail(&self) -> &Trail {
        &self.trail
    }

    pub fn level(&self) -> Option<&Level> {
        self.level.as_deref()
    }

    pub fn checked(&self) -> &[bool] {
        &self.checked
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn show(&mut self, level: Rc<Level>) {
        self.checked = level
            .items
            .iter()
            .map(|i| selectable(i.kind) && i.days_unused >= storage::OLD_DAYS)
            .collect();
        self.cursor = 0;
        self.level = Some(level);
    }

    /// Put the place at the top of the trail on screen: from the cache, from the map's scan, or by scanning.
    fn load_current(&mut self) {
        if let Some((h, _)) = self.level_job.take() {
            h.cancel();
        }
        self.level = None;
        self.checked.clear();
        self.cursor = 0;
        let Some(place) = self.trail.current().cloned() else {
            return;
        };
        if let Some(level) = self.cache.get(&place) {
            self.show(level);
            return;
        }
        match &place {
            Place::Other => {
                if let Some(r) = &self.result {
                    let level = Rc::new(other_level(
                        r,
                        MAP_BLOCKS,
                        SystemTime::now(),
                        self.by_access(),
                        ROW_CAP,
                    ));
                    self.cache.put(place, level.clone());
                    self.show(level);
                }
            }
            Place::Folder(path) => {
                if self.result.is_some() {
                    self.scans += 1;
                    let h = scan::start(path.clone(), self.options(1));
                    self.level_job = Some((h, place));
                }
            }
        }
    }

    fn open_place(&mut self, place: Place) -> bool {
        self.trail.push(place);
        self.load_current();
        true
    }

    /// Click a block of the map.
    pub fn open_block(&mut self, i: usize) -> bool {
        let Some(result) = &self.result else {
            return false;
        };
        let place = match storage::blocks_from(result, MAP_BLOCKS).get(i) {
            Some(b) => match &b.path {
                Some(p) => Place::Folder(p.clone()),
                None => Place::Other,
            },
            None => return false,
        };
        self.open_place(place)
    }

    /// Click a row: folders open.
    pub fn open_row(&mut self, i: usize) -> bool {
        let place = match self.level.as_ref().and_then(|l| l.items.get(i)) {
            Some(item) if item.kind == RowKind::Folder => Place::Folder(item.path.clone()),
            _ => return false,
        };
        self.open_place(place)
    }

    /// Go straight to a folder below the root, with every folder on the way on the trail (so Back
    /// works). False, and nothing changes, if it is not a folder.
    pub fn open_path(&mut self, rel: &Path) -> bool {
        let mut here = self.root.clone();
        let mut places = Vec::new();
        for part in rel.components() {
            let std::path::Component::Normal(name) = part else {
                return false;
            };
            here.push(name);
            let is_folder = std::fs::symlink_metadata(&here).is_ok_and(|m| m.is_dir());
            if !is_folder {
                return false;
            }
            places.push(Place::Folder(here.clone()));
        }
        if places.is_empty() {
            return false;
        }
        self.trail = Trail(places);
        self.load_current();
        true
    }

    pub fn up(&mut self) {
        self.trail.pop();
        self.load_current();
    }

    pub fn jump(&mut self, n: usize) {
        self.trail.jump(n);
        self.load_current();
    }

    pub fn toggle(&mut self, i: usize) {
        let ok = self
            .level
            .as_ref()
            .and_then(|l| l.items.get(i))
            .is_some_and(|item| selectable(item.kind));
        if let (true, Some(on)) = (ok, self.checked.get_mut(i)) {
            *on = !*on;
        }
    }

    pub fn key(&mut self, a: KeyAction) {
        let rows = self.level.as_ref().map_or(0, |l| l.items.len());
        match a {
            KeyAction::Open => {
                self.open_row(self.cursor);
            }
            KeyAction::Toggle => self.toggle(self.cursor),
            KeyAction::Up => {
                if !self.trail.is_empty() {
                    self.up();
                }
            }
            KeyAction::Prev => self.cursor = self.cursor.saturating_sub(1),
            KeyAction::Next => self.cursor = (self.cursor + 1).min(rows.saturating_sub(1)),
        }
    }

    /// These paths were moved or put back: forget the levels that hold them and measure the map again.
    pub fn forget(&mut self, paths: &[PathBuf]) {
        self.cache.invalidate_for(paths);
        if let Some((h, _)) = self.level_job.take() {
            h.cancel();
        }
        if let Some(h) = self.root_job.take() {
            h.cancel();
        }
        self.level = None;
        self.checked.clear();
        self.result = None;
        self.scans += 1;
        self.root_job = Some(scan::start(self.root.clone(), self.options(500)));
    }
}

/// Rows that can be ticked (and moved to the Trash).
pub fn selectable(kind: RowKind) -> bool {
    matches!(kind, RowKind::File | RowKind::Folder | RowKind::Link)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{self, DEEP};
    use std::path::Path;
    use std::time::Duration;

    fn name(p: &str) -> String {
        p.to_string()
    }

    fn folder(p: &str) -> Place {
        Place::Folder(PathBuf::from(p))
    }

    #[test]
    fn the_trail_pushes_pops_and_jumps() {
        let mut t = Trail::default();
        assert!(t.is_empty() && t.current().is_none());
        t.push(folder("/a"));
        t.push(folder("/a/b"));
        t.push(folder("/a/b/c"));
        assert_eq!(t.len(), 3);
        assert_eq!(t.current(), Some(&folder("/a/b/c")));
        t.pop();
        assert_eq!(t.current(), Some(&folder("/a/b")));
        t.push(folder("/a/b/d"));
        t.jump(1);
        assert_eq!(t.places(), &[folder("/a")]);
        t.jump(5);
        assert_eq!(t.len(), 1, "jumping past the end changes nothing");
        t.jump(0);
        assert!(t.is_empty());
        t.pop();
        assert!(t.is_empty(), "popping an empty trail is harmless");
    }

    #[test]
    fn the_breadcrumb_names_every_level_and_links_all_but_the_last() {
        let c = crumbs(
            "Main drive",
            "Your files",
            &[name("Documents"), name("Creative Chaos")],
        );
        let labels: Vec<_> = c.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Main drive", "Your files", "Documents", "Creative Chaos"]
        );
        let targets: Vec<_> = c.iter().map(|c| c.target).collect();
        assert_eq!(targets, [Some(0), Some(0), Some(1), None]);
    }

    #[test]
    fn at_the_map_the_breadcrumb_is_just_the_drive_and_folder() {
        let c = crumbs("Main drive", "Your files", &[]);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].target, None);
    }

    fn six() -> Vec<Crumb> {
        crumbs(
            "Main drive",
            "Your files",
            &[
                name("Documents"),
                name("Creative Chaos"),
                name("OpenSource"),
                name("tidewise"),
            ],
        )
    }

    #[test]
    fn a_long_breadcrumb_keeps_the_first_and_the_last_two() {
        let c = collapse(six(), 40);
        let labels: Vec<_> = c.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Main drive", "…", "OpenSource", "tidewise"]);
        // The dots go back to the level just before the last two.
        assert_eq!(c[1].target, Some(2));
        assert_eq!(c[2].target, Some(3));
        assert_eq!(c[3].target, None);
    }

    #[test]
    fn a_breadcrumb_that_fits_is_left_alone() {
        let full = six();
        let width: usize =
            full.iter().map(|c| c.label.chars().count()).sum::<usize>() + 3 * (full.len() - 1);
        assert_eq!(
            collapse(six(), width),
            full,
            "exactly at the limit still fits"
        );
        assert_ne!(collapse(six(), width - 1), full, "one over collapses");
        assert_eq!(collapse(Vec::new(), 1), Vec::new());
    }

    #[test]
    fn a_breadcrumb_of_three_or_fewer_parts_never_collapses() {
        let c = crumbs(
            "Main drive",
            "Your files",
            &[name("A very long folder name indeed")],
        );
        assert_eq!(collapse(c.clone(), 5), c);
    }

    #[test]
    fn the_cache_hands_back_what_it_was_given() {
        let mut c = Cache::default();
        let level = Rc::new(Level {
            total: 7,
            ..Level::default()
        });
        c.put(folder("/r/a"), level.clone());
        assert_eq!(c.get(&folder("/r/a")).unwrap().total, 7);
        assert!(c.get(&folder("/r/b")).is_none());
        c.clear();
        assert!(c.is_empty());
    }

    #[test]
    fn a_change_drops_the_levels_that_hold_the_path_and_no_others() {
        let mut c = Cache::default();
        let l = Rc::new(Level::default());
        for p in ["/r/a", "/r/a/b", "/r/a/b/c", "/r/a2", "/r/z"] {
            c.put(folder(p), l.clone());
        }
        c.put(Place::Other, l.clone());
        c.invalidate_for(&[PathBuf::from("/r/a/b/file.iso")]);
        assert!(c.get(&folder("/r/a/b")).is_none(), "its own folder");
        assert!(c.get(&folder("/r/a")).is_none(), "its parent");
        assert!(c.get(&Place::Other).is_none(), "Other may hold it");
        assert!(
            c.get(&folder("/r/a/b/c")).is_some(),
            "a folder below is untouched"
        );
        assert!(
            c.get(&folder("/r/a2")).is_some(),
            "/r/a2 is not inside /r/a"
        );
        assert!(c.get(&folder("/r/z")).is_some());
    }

    #[test]
    fn keys_map_to_actions() {
        assert_eq!(key_action("\n", false), Some(KeyAction::Open));
        assert_eq!(key_action(" ", false), Some(KeyAction::Toggle));
        assert_eq!(key_action("\u{8}", false), Some(KeyAction::Up));
        assert_eq!(
            key_action("\u{F702}", true),
            Some(KeyAction::Up),
            "Alt+Left"
        );
        assert_eq!(
            key_action("\u{F702}", false),
            None,
            "Left alone does nothing"
        );
        assert_eq!(key_action("\u{F700}", false), Some(KeyAction::Prev));
        assert_eq!(key_action("\u{F701}", false), Some(KeyAction::Next));
        assert_eq!(key_action("a", false), None);
        assert_eq!(key_action("", false), None);
    }

    // --- levels, from the fixture tree on a real disk ---

    fn scan(dir: &Path) -> ScanResult {
        tidewise_core::scan::scan(
            dir,
            &tidewise_core::scan::ScanOptions::default(),
            &tidewise_core::scan::Progress::default(),
            &std::sync::atomic::AtomicBool::new(false),
        )
    }

    fn now() -> SystemTime {
        SystemTime::now()
    }

    #[test]
    fn a_level_lists_every_child_biggest_first_with_its_kind() {
        let fx = fixture::build();
        let level = level_from(&scan(&fx.root), now(), true, ROW_CAP);
        let names: Vec<_> = level.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names[0], "Documents");
        let kind = |n: &str| level.items.iter().find(|i| i.name == n).unwrap().kind;
        assert_eq!(kind("Documents"), RowKind::Folder);
        assert_eq!(
            kind("shortcut"),
            RowKind::Link,
            "a symlink is a row that does not open"
        );
        assert!(level.items.windows(2).all(|w| w[0].bytes >= w[1].bytes));
        let photos = level_from(
            &scan(&fx.root.join("Documents/Photos")),
            now(),
            true,
            ROW_CAP,
        );
        assert_eq!(photos.items.len(), 45);
        assert!(photos.items.iter().all(|i| i.kind == RowKind::File));
        assert_eq!(
            photos.total,
            photos.items.iter().map(|i| i.bytes).sum::<u64>()
        );
    }

    #[test]
    fn a_folder_on_another_drive_is_a_row_that_does_not_open() {
        let level = level_from(&scan(Path::new("/dev")), now(), true, ROW_CAP);
        if Path::new("/dev/shm").is_dir() {
            let row = level
                .items
                .iter()
                .find(|i| i.name == "shm")
                .expect("shm row");
            assert_eq!(row.kind, RowKind::OtherDrive);
            assert_eq!(row.bytes, 0);
        }
    }

    #[test]
    fn old_things_are_marked_by_how_long_ago_they_were_used() {
        let fx = fixture::build();
        let level = level_from(&scan(&fx.root.join("Downloads")), now(), true, ROW_CAP);
        let days = |n: &str| {
            level
                .items
                .iter()
                .find(|i| i.name == n)
                .unwrap()
                .days_unused
        };
        assert!(days("installer.iso") >= 399 && days("installer.iso") <= 401);
        assert!(days("old.zip") >= 299 && days("old.zip") <= 301);
    }

    #[test]
    fn other_lists_the_folders_the_map_grouped_together() {
        let fx = fixture::build();
        let root = scan(&fx.root);
        let blocks = crate::storage::blocks_from(&root, 6);
        let other = blocks.last().unwrap();
        assert_eq!(other.name, "Other");
        let level = other_level(&root, 6, now(), true, ROW_CAP);
        assert_eq!(
            level.items.len(),
            root.children.iter().filter(|c| c.1 > 0).count() - 5
        );
        assert_eq!(level.total, other.bytes, "the block and its list agree");
    }

    fn kids(n: usize) -> Vec<(PathBuf, u64)> {
        (0..n)
            .map(|i| (PathBuf::from(format!("/r/{i}")), 1_000 - i as u64 % 1_000))
            .collect()
    }

    #[test]
    fn a_list_is_cut_at_the_cap_and_the_rest_is_counted() {
        let k = kids(2_500);
        let (shown, rest) = split_cap(&k, ROW_CAP);
        assert_eq!(shown.len(), 2_000);
        let left: u64 = k[2_000..].iter().map(|c| c.1).sum();
        assert_eq!(rest, Some((500, left)));
    }

    #[test]
    fn a_list_at_or_under_the_cap_is_shown_whole() {
        for n in [0, 1, 1_999, 2_000] {
            let k = kids(n);
            let (shown, rest) = split_cap(&k, ROW_CAP);
            assert_eq!((shown.len(), rest), (n, None), "{n} children");
        }
        let k = kids(2_001);
        assert_eq!(split_cap(&k, ROW_CAP).1.map(|r| r.0), Some(1));
    }

    #[test]
    fn a_level_keeps_the_biggest_rows_and_its_real_total() {
        let fx = fixture::build();
        let r = scan(&fx.root.join("Documents/Photos"));
        let level = level_from(&r, now(), true, 10);
        assert_eq!(level.items.len(), 10);
        let left: u64 = r.children[10..].iter().map(|c| c.1).sum();
        assert_eq!(level.rest, Some((35, left)));
        assert_eq!(
            level.total, r.total_bytes,
            "the total still counts everything"
        );
        assert_eq!(level.items[0].bytes, r.children[0].1, "biggest first");
    }

    #[test]
    fn the_last_row_says_how_many_smaller_items_were_left_out() {
        assert_eq!(
            rest_text(35, 12_000_000),
            "and 35 smaller items \u{b7} 12 MB"
        );
        assert_eq!(rest_text(1, 3_000_000), "and 1 smaller item \u{b7} 3 MB");
    }

    // --- the browser ---

    fn wait(b: &mut Browser) {
        let end = std::time::Instant::now() + Duration::from_secs(30);
        while !b.is_idle() {
            b.poll();
            assert!(std::time::Instant::now() < end, "scan did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        b.poll();
    }

    fn browser(fx: &fixture::Fixture) -> Browser {
        let mut b = Browser::new(fx.root.clone());
        b.start();
        wait(&mut b);
        b
    }

    fn row_of(b: &Browser, name: &str) -> usize {
        b.level()
            .unwrap()
            .items
            .iter()
            .position(|i| i.name == name)
            .unwrap()
    }

    fn open_path(b: &mut Browser, rel: &str) {
        for part in rel.split('/') {
            let i = row_of(b, part);
            assert!(b.open_row(i), "{part} should open");
            wait(b);
        }
    }

    #[test]
    fn a_block_opens_and_folders_inside_it_open_further() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        assert!(b.trail().is_empty() && b.level().is_none());
        assert!(b.open_block(0)); // Documents, the biggest
        wait(&mut b);
        assert_eq!(
            b.trail().current(),
            Some(&Place::Folder(fx.root.join("Documents")))
        );
        open_path(&mut b, "Creative Chaos/OpenSource/tidewise");
        assert_eq!(b.trail().len(), 4);
        assert!(b
            .level()
            .unwrap()
            .items
            .iter()
            .any(|i| i.name == "build.bin"));
        let _ = DEEP;
    }

    #[test]
    fn a_deep_path_opens_with_every_folder_on_the_way() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        assert!(b.open_path(Path::new(DEEP)));
        wait(&mut b);
        let want: Vec<Place> = [
            "Documents",
            "Documents/Creative Chaos",
            "Documents/Creative Chaos/OpenSource",
            DEEP,
        ]
        .iter()
        .map(|p| Place::Folder(fx.root.join(p)))
        .collect();
        assert_eq!(b.trail().places(), want.as_slice());
        assert!(b
            .level()
            .unwrap()
            .items
            .iter()
            .any(|i| i.name == "build.bin"));
        b.up();
        wait(&mut b);
        assert_eq!(b.trail().len(), 3, "Back works");
        assert!(!b.open_path(Path::new("Nope/Nothing")));
        assert!(
            !b.open_path(Path::new("Documents/Archive/disk.img")),
            "a file is not a folder"
        );
        assert_eq!(b.trail().len(), 3, "a refused path changes nothing");
    }

    #[test]
    fn files_and_links_do_not_open() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        b.open_block(0);
        wait(&mut b);
        let n = b.trail().len();
        let disk = row_of(&b, "Archive");
        b.open_row(disk);
        wait(&mut b);
        let img = row_of(&b, "disk.img");
        assert!(!b.open_row(img));
        assert_eq!(b.trail().len(), n + 1);
    }

    #[test]
    fn going_up_and_jumping_walk_back_through_the_trail() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        b.open_block(0);
        wait(&mut b);
        open_path(&mut b, "Creative Chaos/OpenSource");
        b.up();
        wait(&mut b);
        assert_eq!(b.trail().len(), 2);
        b.jump(0);
        assert!(
            b.trail().is_empty() && b.level().is_none(),
            "back at the map"
        );
    }

    #[test]
    fn going_back_is_instant_because_levels_are_cached() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        b.open_block(0);
        wait(&mut b);
        open_path(&mut b, "Photos");
        let scans = b.scans_started();
        b.up();
        assert!(
            b.is_idle() && b.level().is_some(),
            "Documents is on screen at once"
        );
        let again = row_of(&b, "Photos");
        b.open_row(again);
        assert!(b.is_idle() && b.level().is_some(), "and so is Photos");
        assert_eq!(b.scans_started(), scans, "no new scan");
    }

    #[test]
    fn ticks_belong_to_the_open_folder_and_are_cleared_on_leaving() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        b.open_block(0);
        wait(&mut b);
        open_path(&mut b, "Photos");
        assert!(b.checked().iter().all(|on| !on), "nothing in Photos is old");
        b.toggle(3);
        b.toggle(4);
        assert_eq!(b.checked().iter().filter(|on| **on).count(), 2);
        b.up();
        open_path(&mut b, "Photos");
        assert!(
            b.checked().iter().all(|on| !on),
            "ticks did not survive leaving"
        );
        b.toggle(0);
        b.toggle(0);
        assert!(!b.checked()[0], "ticking twice unticks");
    }

    #[test]
    fn old_things_are_ticked_when_a_level_opens_at_every_level() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        let downloads = b.level().is_none() as usize; // 1, at the map
        let _ = downloads;
        let tile = crate::storage::blocks_from(&scan(&fx.root), 6)
            .iter()
            .position(|t| t.name == "Downloads")
            .unwrap();
        b.open_block(tile);
        wait(&mut b);
        let items = &b.level().unwrap().items;
        for (item, on) in items.iter().zip(b.checked()) {
            assert_eq!(
                *on,
                item.days_unused >= crate::storage::OLD_DAYS,
                "{}",
                item.name
            );
        }
        assert_eq!(b.checked().iter().filter(|on| **on).count(), 2);
    }

    #[test]
    fn the_other_block_opens_its_members_and_they_open_too() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        let last = crate::storage::blocks_from(&scan(&fx.root), 6).len() - 1;
        assert!(b.open_block(last));
        assert_eq!(b.trail().current(), Some(&Place::Other));
        let member = b
            .level()
            .unwrap()
            .items
            .iter()
            .position(|i| i.kind == RowKind::Folder)
            .unwrap();
        assert!(b.open_row(member));
        wait(&mut b);
        b.up();
        assert_eq!(
            b.trail().current(),
            Some(&Place::Other),
            "back to the Other list"
        );
    }

    #[test]
    fn a_trash_forgets_only_the_levels_that_hold_the_file() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        b.open_block(0);
        wait(&mut b);
        open_path(&mut b, "Photos");
        b.up();
        b.jump(0);
        let tile = crate::storage::blocks_from(&scan(&fx.root), 6)
            .iter()
            .position(|t| t.name == "Downloads")
            .unwrap();
        b.open_block(tile);
        wait(&mut b);
        b.jump(0);
        let before = b.scans_started();
        b.forget(&[fx.root.join("Documents/Photos/photo-00.jpg")]);
        wait(&mut b);
        let after_forget = b.scans_started();
        assert!(after_forget > before, "the map is measured again");
        // Downloads was not touched: opening it again needs no scan.
        b.open_block(tile);
        assert!(b.is_idle() && b.level().is_some());
        assert_eq!(b.scans_started(), after_forget);
        // Photos was: opening it scans again.
        b.jump(0);
        b.open_block(0);
        wait(&mut b);
        let photos = row_of(&b, "Photos");
        b.open_row(photos);
        wait(&mut b);
        assert!(b.scans_started() > after_forget);
    }

    #[test]
    fn keys_move_open_tick_and_go_up() {
        let fx = fixture::build();
        let mut b = browser(&fx);
        b.open_block(0);
        wait(&mut b);
        assert_eq!(b.cursor(), 0);
        b.key(KeyAction::Next);
        assert_eq!(b.cursor(), 1);
        b.key(KeyAction::Prev);
        b.key(KeyAction::Prev);
        assert_eq!(b.cursor(), 0, "stops at the top");
        let photos = row_of(&b, "Photos");
        for _ in 0..photos {
            b.key(KeyAction::Next);
        }
        b.key(KeyAction::Open);
        wait(&mut b);
        assert_eq!(b.trail().len(), 2);
        assert_eq!(b.cursor(), 0, "a new level starts at its first row");
        b.key(KeyAction::Toggle);
        assert!(b.checked()[0]);
        b.key(KeyAction::Up);
        assert_eq!(b.trail().len(), 1);
        let n = b.level().unwrap().items.len();
        for _ in 0..n + 3 {
            b.key(KeyAction::Next);
        }
        assert_eq!(b.cursor(), n - 1, "stops at the bottom");
    }
}
