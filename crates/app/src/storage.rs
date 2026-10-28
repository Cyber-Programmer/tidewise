//! Data behind the Storage screen. Plain Rust, no Slint types.

use std::path::{Path, PathBuf};

use tidewise_core::scan::ScanResult;

const GB: f64 = 1_000_000_000.0;
const MB: f64 = 1_000_000.0;

/// A rectangle as fractions (0.0–1.0) of the map it sits in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A size as the design writes it: `381 GB`, `12.6 GB`, `340 MB`, `2 TB`. Whole numbers lose the
/// `.0`; from 100 GB up there are no decimals.
pub fn fmt_size(_bytes: u64) -> String {
    String::new()
}

/// Always one decimal in GB (`6.1 GB`, `22.0 GB`): the file rows and the Trash button.
pub fn fmt_gb1(_bytes: u64) -> String {
    String::new()
}

/// How long ago, as a plain amount: `3 weeks`, `8 months`, `1 year`. None for under two days.
pub fn age_amount(_days: u64) -> Option<String> {
    None
}

/// `yesterday`, `3 weeks ago`, `8 months ago`.
pub fn age_ago(_days: u64) -> String {
    String::new()
}

/// Squarified treemap: `sizes` (biggest first) laid out in a box `aspect` times as wide as tall.
/// Returns one rectangle per size, in order, covering the box with no overlap.
pub fn treemap(_sizes: &[u64], _aspect: f64) -> Vec<Rect> {
    Vec::new()
}

/// One block of the folder map.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub name: String,
    pub path: Option<PathBuf>,
    pub bytes: u64,
}

/// The folder map's blocks from a scan: the biggest `max - 1` folders plus "Other" for the rest.
pub fn blocks_from(_scan: &ScanResult, _max: usize) -> Vec<Block> {
    Vec::new()
}

/// "Downloads · opened 8 months ago": the first folder under home, then how long ago.
pub fn file_meta(_home: &Path, _path: &Path, _days_since_use: u64) -> String {
    String::new()
}

/// What the Move button says: `Move 6 to Trash · 22.0 GB`; names nothing when nothing is ticked.
pub fn trash_label(_count: usize, _bytes: u64) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 1;

    #[test]
    fn sizes_are_written_like_the_design() {
        assert_eq!(fmt_size(381_000_000_000), "381 GB");
        assert_eq!(fmt_size(12_600_000_000), "12.6 GB");
        assert_eq!(fmt_size(52_000_000_000), "52 GB");
        assert_eq!(fmt_size(142_300_000_000), "142 GB");
        assert_eq!(fmt_size(340_000_000), "340 MB");
        assert_eq!(fmt_size(2_000_000_000_000), "2 TB");
        assert_eq!(fmt_size(0), "0 MB");
    }

    #[test]
    fn file_sizes_always_have_one_decimal() {
        assert_eq!(fmt_gb1(6_100_000_000), "6.1 GB");
        assert_eq!(fmt_gb1(22_000_000_000), "22.0 GB");
        assert_eq!(fmt_gb1(100_000_000), "0.1 GB");
        assert_eq!(fmt_gb1(12_449_000_000), "12.4 GB");
    }

    #[test]
    fn ages_read_like_the_design() {
        assert_eq!(age_amount(1), None);
        assert_eq!(age_ago(0), "today");
        assert_eq!(age_ago(DAY), "yesterday");
        assert_eq!(age_ago(5), "5 days ago");
        assert_eq!(age_ago(21), "3 weeks ago");
        assert_eq!(age_ago(8 * 30 + 5), "8 months ago");
        assert_eq!(age_ago(365), "1 year ago");
        assert_eq!(age_ago(800), "2 years ago");
        assert_eq!(age_amount(21).as_deref(), Some("3 weeks"));
        assert_eq!(age_amount(300).as_deref(), Some("10 months"));
        assert_eq!(age_amount(400).as_deref(), Some("1 year"));
    }

    #[test]
    fn age_boundaries_do_not_skip_a_unit() {
        assert_eq!(age_ago(13), "13 days ago");
        assert_eq!(age_ago(14), "2 weeks ago");
        assert_eq!(age_ago(59), "8 weeks ago");
        assert_eq!(age_ago(60), "2 months ago");
        assert_eq!(age_ago(359), "11 months ago");
        assert_eq!(age_ago(360), "1 year ago");
    }

    fn area(r: &Rect) -> f64 {
        r.w * r.h
    }

    #[test]
    fn treemap_areas_are_proportional_and_fill_the_box() {
        let sizes = [52, 38, 21, 12, 10, 9];
        let rects = treemap(&sizes, 2.0);
        assert_eq!(rects.len(), 6);
        let total: f64 = rects.iter().map(area).sum();
        assert!((total - 1.0).abs() < 1e-9, "fills the box: {total}");
        for (r, s) in rects.iter().zip(sizes) {
            assert!((area(r) - s as f64 / 142.0).abs() < 1e-9, "{r:?} for {s}");
            assert!(r.x >= -1e-9 && r.y >= -1e-9 && r.x + r.w <= 1.0 + 1e-9 && r.y + r.h <= 1.0 + 1e-9);
        }
    }

    #[test]
    fn treemap_blocks_do_not_overlap() {
        let rects = treemap(&[50, 30, 30, 20, 10, 5, 5, 1], 1.5);
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let ox = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let oy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(ox <= 1e-9 || oy <= 1e-9, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn treemap_matches_the_artboard_layout() {
        // Steam is a tall block on the left (36.6 % wide); Downloads and Videos share the top
        // 66 % of the rest; Pictures, Other and Documents the bottom 34 %.
        let r = treemap(&[52, 38, 21, 12, 10, 9], 683.0 / 340.0);
        assert!((r[0].w - 0.366).abs() < 0.01 && (r[0].h - 1.0).abs() < 1e-9, "{:?}", r[0]);
        assert!((r[1].y - 0.0).abs() < 1e-9 && (r[1].h - 0.656).abs() < 0.02, "{:?}", r[1]);
        assert!((r[2].y - 0.0).abs() < 1e-9, "{:?}", r[2]);
        for i in 3..6 {
            assert!((r[i].y - 0.656).abs() < 0.02, "{:?}", r[i]);
        }
    }

    #[test]
    fn treemap_handles_nothing_zero_and_one() {
        assert!(treemap(&[], 2.0).is_empty());
        assert_eq!(treemap(&[5], 2.0), vec![Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }]);
        let r = treemap(&[10, 0, 0], 2.0);
        assert_eq!(r.len(), 3);
        assert!(r.iter().all(|r| r.w.is_finite() && r.h.is_finite()));
    }

    fn scan_with(children: &[(&str, u64)]) -> ScanResult {
        ScanResult {
            root: "/home/ali".into(),
            total_bytes: children.iter().map(|c| c.1).sum(),
            children: children
                .iter()
                .map(|(n, b)| (PathBuf::from("/home/ali").join(n), *b))
                .collect(),
            ..ScanResult::default()
        }
    }

    #[test]
    fn the_map_keeps_the_biggest_folders_and_groups_the_rest_as_other() {
        let scan = scan_with(&[("a", 50), ("b", 40), ("c", 30), ("d", 20), ("e", 10)]);
        let blocks = blocks_from(&scan, 4);
        let names: Vec<_> = blocks.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c", "Other"]);
        assert_eq!(blocks[3].bytes, 30);
        assert!(blocks[3].path.is_none());
        assert_eq!(blocks.iter().map(|b| b.bytes).sum::<u64>(), 150);
    }

    #[test]
    fn a_short_list_has_no_other_and_empty_folders_are_dropped() {
        let scan = scan_with(&[("a", 50), ("b", 0)]);
        let blocks = blocks_from(&scan, 6);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "a");
        assert!(blocks_from(&ScanResult::default(), 6).is_empty());
    }

    #[test]
    fn hidden_folders_get_plain_names() {
        let scan = scan_with(&[(".cache", 50), (".local", 40), (".weird-tool", 30)]);
        let names: Vec<_> = blocks_from(&scan, 6).into_iter().map(|b| b.name).collect();
        assert_eq!(names, ["Caches", "App data", ".weird-tool"]);
    }

    #[test]
    fn file_meta_names_the_folder_under_home() {
        let home = Path::new("/home/ali");
        assert_eq!(
            file_meta(home, Path::new("/home/ali/Downloads/x.iso"), 245),
            "Downloads · opened 8 months ago"
        );
        assert_eq!(
            file_meta(home, Path::new("/home/ali/Projects/a/b/node_modules/x"), 21),
            "Projects · opened 3 weeks ago"
        );
        assert_eq!(
            file_meta(home, Path::new("/home/ali/notes.txt"), 0),
            "Home folder · opened today"
        );
        assert_eq!(
            file_meta(home, Path::new("/elsewhere/x"), 1),
            "elsewhere · opened yesterday"
        );
    }

    #[test]
    fn the_trash_button_names_what_it_does() {
        assert_eq!(trash_label(6, 22_000_000_000), "Move 6 to Trash · 22.0 GB");
        assert_eq!(trash_label(1, 6_100_000_000), "Move 1 to Trash · 6.1 GB");
        assert_eq!(trash_label(0, 0), "Nothing ticked");
    }
}
