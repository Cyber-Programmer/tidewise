//! Going as deep as you like in Storage: the path stack, breadcrumbs, scanned levels and their
//! cache, ticks and keys. Plain Rust, no Slint types, so it is tested without a window.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use tidewise_core::scan::ScanResult;

use crate::storage::RowKind;

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
    pub fn push(&mut self, _place: Place) {}
    pub fn pop(&mut self) {}
    /// Keep the first `n` places.
    pub fn jump(&mut self, _n: usize) {}
    pub fn len(&self) -> usize {
        0
    }
    pub fn is_empty(&self) -> bool {
        true
    }
    pub fn current(&self) -> Option<&Place> {
        None
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
pub fn crumbs(_drive: &str, _files: &str, _trail: &[String]) -> Vec<Crumb> {
    Vec::new()
}

/// Shorten a breadcrumb that is longer than `max_chars` (parts joined by ` › `): keep the first
/// part and the last two, put `…` in between.
pub fn collapse(c: Vec<Crumb>, _max_chars: usize) -> Vec<Crumb> {
    c
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

/// What one opened folder holds, biggest first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Level {
    pub items: Vec<Item>,
    pub total: u64,
}

pub fn level_from(_r: &ScanResult, _now: SystemTime, _by_access: bool) -> Level {
    Level::default()
}

/// The folders the map groups into "Other" (all but the biggest `max_blocks - 1`).
pub fn other_level(
    _root: &ScanResult,
    _max_blocks: usize,
    _now: SystemTime,
    _by_access: bool,
) -> Level {
    Level::default()
}

/// Levels already scanned, so going back is instant.
#[derive(Default)]
pub struct Cache(HashMap<Place, Rc<Level>>);

impl Cache {
    pub fn put(&mut self, _place: Place, _level: Rc<Level>) {}
    pub fn get(&self, _place: &Place) -> Option<Rc<Level>> {
        None
    }
    /// Forget every level that holds one of these paths (its own size changed).
    pub fn invalidate_for(&mut self, _paths: &[PathBuf]) {}
    pub fn clear(&mut self) {}
    pub fn len(&self) -> usize {
        0
    }
    pub fn is_empty(&self) -> bool {
        true
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
pub fn key_action(_text: &str, _alt: bool) -> Option<KeyAction> {
    None
}

/// The Storage page without any window: the map's scan, where you are, what is ticked.
pub struct Browser {
    _root: PathBuf,
}

impl Browser {
    pub fn new(root: PathBuf) -> Self {
        Browser { _root: root }
    }
    pub fn start(&mut self) {}
    /// Collect finished scans. True when something on screen changed.
    pub fn poll(&mut self) -> bool {
        false
    }
    pub fn is_idle(&self) -> bool {
        true
    }
    pub fn scans_started(&self) -> usize {
        0
    }
    pub fn trail(&self) -> &Trail {
        unimplemented!()
    }
    pub fn level(&self) -> Option<&Level> {
        None
    }
    pub fn checked(&self) -> &[bool] {
        &[]
    }
    pub fn cursor(&self) -> usize {
        0
    }
    /// Click a block of the map.
    pub fn open_block(&mut self, _i: usize) -> bool {
        false
    }
    /// Click a row: folders open.
    pub fn open_row(&mut self, _i: usize) -> bool {
        false
    }
    pub fn up(&mut self) {}
    pub fn jump(&mut self, _n: usize) {}
    pub fn toggle(&mut self, _i: usize) {}
    pub fn key(&mut self, _a: KeyAction) {}
    /// These paths were moved or put back: forget the levels that hold them and measure again.
    pub fn forget(&mut self, _paths: &[PathBuf]) {}
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
            items: Vec::new(),
            total: 7,
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
        let level = level_from(&scan(&fx.root), now(), true);
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
        let photos = level_from(&scan(&fx.root.join("Documents/Photos")), now(), true);
        assert_eq!(photos.items.len(), 45);
        assert!(photos.items.iter().all(|i| i.kind == RowKind::File));
        assert_eq!(
            photos.total,
            photos.items.iter().map(|i| i.bytes).sum::<u64>()
        );
    }

    #[test]
    fn a_folder_on_another_drive_is_a_row_that_does_not_open() {
        let level = level_from(&scan(Path::new("/dev")), now(), true);
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
        let level = level_from(&scan(&fx.root.join("Downloads")), now(), true);
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
        let level = other_level(&root, 6, now(), true);
        assert_eq!(
            level.items.len(),
            root.children.iter().filter(|c| c.1 > 0).count() - 5
        );
        assert_eq!(level.total, other.bytes, "the block and its list agree");
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
