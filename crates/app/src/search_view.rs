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
pub fn rows(items: &[Item], hits: &[usize], selected: usize) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    let mut last = None;
    for item in hits.iter().filter_map(|&i| items.get(i)) {
        let (header, icon) = match item.group {
            Group::Apps => ("Apps", 0),
            Group::Processes => ("Processes", 1),
            Group::Actions => ("Actions", 2),
            Group::Settings => ("Settings", 3),
        };
        let first = last != Some(item.group);
        last = Some(item.group);
        let on = out.len() == selected;
        out.push(Row {
            header: if first { header.into() } else { String::new() },
            title: item.title.clone(),
            meta: item.meta.clone(),
            hint: if on { "Enter" } else { "Open" }.into(),
            icon,
            selected: on,
        });
    }
    out
}

/// Move the pick by `delta` rows without leaving the list (no wrapping).
pub fn moved(selected: usize, delta: i32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let at = selected.min(count - 1) as i64 + i64::from(delta);
    at.clamp(0, count as i64 - 1) as usize
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
