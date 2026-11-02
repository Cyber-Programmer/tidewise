//! The rows of the search box: a heading above the first result of each group, the picked row
//! marked. Plain data; `view.rs` copies it into Slint.

use tidewise_core::search::{Group, Item};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// `Apps`, `Processes`, `Actions`, `Settings` on the first row of a group, else empty.
    pub header: String,
    pub title: String,
    pub meta: String,
    /// `Enter` on the picked row, `Open` on the others.
    pub hint: String,
    /// 0 apps, 1 processes, 2 actions, 3 settings.
    pub icon: i32,
    pub selected: bool,
}

/// The rows for `hits` (indices into `items`, in display order); `selected` counts rows.
pub fn rows(_items: &[Item], _hits: &[usize], _selected: usize) -> Vec<Row> {
    Vec::new()
}

/// Move the pick by `delta` rows without leaving the list (no wrapping).
pub fn moved(_selected: usize, _delta: i32, _count: usize) -> usize {
    usize::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(group: Group, title: &str) -> Item {
        Item {
            group,
            title: title.into(),
            meta: format!("{title} meta"),
            keywords: String::new(),
            key: String::new(),
        }
    }

    fn items() -> Vec<Item> {
        vec![
            item(Group::Apps, "Firefox"),
            item(Group::Apps, "Steam"),
            item(Group::Processes, "Firefox"),
            item(Group::Actions, "Clean up"),
            item(Group::Settings, "Theme"),
        ]
    }

    #[test]
    fn the_first_row_of_each_group_carries_its_heading() {
        let r = rows(&items(), &[0, 1, 2, 3, 4], 0);
        let headers: Vec<_> = r.iter().map(|r| r.header.as_str()).collect();
        assert_eq!(headers, ["Apps", "", "Processes", "Actions", "Settings"]);
        let icons: Vec<_> = r.iter().map(|r| r.icon).collect();
        assert_eq!(icons, [0, 0, 1, 2, 3]);
        assert_eq!(r[0].title, "Firefox");
        assert_eq!(r[0].meta, "Firefox meta");
    }

    #[test]
    fn only_the_picked_row_says_enter() {
        let r = rows(&items(), &[0, 2, 4], 1);
        assert_eq!(
            r.iter()
                .map(|r| (r.selected, r.hint.as_str()))
                .collect::<Vec<_>>(),
            [(false, "Open"), (true, "Enter"), (false, "Open")]
        );
    }

    #[test]
    fn no_hits_no_rows_and_bad_indices_are_skipped() {
        assert!(rows(&items(), &[], 0).is_empty());
        let r = rows(&items(), &[0, 99], 0);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn moving_stays_inside_the_list() {
        assert_eq!(moved(0, 1, 5), 1);
        assert_eq!(moved(4, 1, 5), 4, "no wrapping at the end");
        assert_eq!(moved(0, -1, 5), 0, "no wrapping at the start");
        assert_eq!(moved(2, -1, 5), 1);
        assert_eq!(moved(0, 3, 0), 0, "an empty list");
        assert_eq!(moved(9, 0, 3), 2, "a stale pick is brought back");
    }
}
