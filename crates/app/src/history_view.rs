//! The History screen: what the rows, the details and the Undo button say, from the records in
//! `$XDG_STATE_HOME/tidewise/history/`. Plain data; `view.rs` copies it into Slint.

use tidewise_core::history::{self, Kind, Record, Status, Undo};

/// Which icon a row gets (the artboard's colours per kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Clean,
    App,
    Startup,
    Trash,
    Process,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub title: String,
    /// `19:41 · 4 groups cleaned`
    pub subtitle: String,
    pub icon: Icon,
    /// `Restore`, `No undo needed` or empty.
    pub hint: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    pub label: String,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    pub id: String,
    pub title: String,
    /// `Done`, `Partly done`, `Failed`, `Skipped`, `Forced`.
    pub result: String,
    /// `Today, 19:41`
    pub when: String,
    /// What changed: label and value.
    pub lines: Vec<(String, String)>,
    pub note: String,
    /// Behind "Show technical details".
    pub log: String,
    /// The Undo button's words; `None` when there is nothing to undo without admin access.
    pub undo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Screen {
    pub days: Vec<Day>,
    pub detail: Option<Detail>,
}

/// Build the screen. `selected` is a record id; when it is unknown the newest record is shown.
pub fn screen(
    _records: Vec<Record>,
    _selected: Option<&str>,
    _now: u64,
    _offset_secs: i64,
) -> Screen {
    Screen::default()
}

#[allow(dead_code)]
fn unused(_: Kind, _: Status, _: Undo) {
    let _ = history::VERSION;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewise_core::history::{Item, ItemResult, Step};

    /// 2026-11-02 12:00:00 UTC.
    const NOW: u64 = 1_793_620_800;

    fn step(title: &str, status: Status, bytes: u64) -> Step {
        Step {
            title: title.into(),
            status,
            bytes,
            items: vec![],
            entries: vec![],
            note: None,
        }
    }

    fn rec(id: &str, time: u64, kind: Kind, title: &str, steps: Vec<Step>) -> Record {
        Record {
            version: history::VERSION,
            id: id.into(),
            time,
            kind,
            title: title.into(),
            freed_bytes: steps.iter().map(|s| s.bytes).sum(),
            steps,
        }
    }

    fn clean_up(id: &str, time: u64) -> Record {
        rec(
            id,
            time,
            Kind::CleanUp,
            "Cleaned 10.1 GB",
            vec![
                step("App caches", Status::Done, 3_100_000_000),
                step("Old system logs", Status::Skipped, 0),
                step("Trash", Status::Done, 7_000_000_000),
            ],
        )
    }

    fn trash(id: &str, time: u64, tmp: &std::path::Path) -> Record {
        let home = tmp.join("home");
        let dir = home.join(".local/share/Trash");
        std::fs::create_dir_all(home.join("Downloads")).unwrap();
        let mut moved = Vec::new();
        for (name, size) in [("ubuntu.iso", 6_100_000_000u64), ("old.zip", 1_600_000_000)] {
            let p = home.join("Downloads").join(name);
            std::fs::write(&p, b"x").unwrap();
            let t = tidewise_core::trash::move_to_trash(
                &p,
                &dir,
                &home,
                std::time::SystemTime::UNIX_EPOCH,
            )
            .unwrap();
            moved.push((t, size));
        }
        history::trash_record(&moved, time, id)
    }

    #[test]
    fn no_records_is_an_empty_screen() {
        let s = screen(vec![], None, NOW, 0);
        assert!(s.days.is_empty() && s.detail.is_none());
    }

    #[test]
    fn rows_are_grouped_by_day_with_time_summary_and_the_newest_selected() {
        let tmp = tempfile::tempdir().unwrap();
        let s = screen(
            vec![
                clean_up("a", NOW - 3600),
                trash("b", NOW - 86_400, tmp.path()),
                rec(
                    "c",
                    NOW - 120,
                    Kind::EndTask,
                    "Ended Firefox",
                    vec![
                        history::end_task_record("Firefox", 14, 2_300_000_000, false, 0, "x")
                            .steps
                            .remove(0),
                    ],
                ),
            ],
            None,
            NOW,
            0,
        );
        let labels: Vec<_> = s.days.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, ["Today", "Yesterday"]);
        let today = &s.days[0].rows;
        assert_eq!(
            today.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["c", "a"]
        );
        assert!(
            today[0].selected && !today[1].selected,
            "the newest is picked"
        );
        assert_eq!(today[0].title, "Ended Firefox");
        assert_eq!(today[0].subtitle, "11:58 · 14 processes ended");
        assert_eq!(today[0].icon, Icon::Process);
        assert_eq!(today[1].subtitle, "11:00 · 2 groups cleaned · 1 skipped");
        assert_eq!(
            (today[1].icon, today[1].hint.as_str()),
            (Icon::Clean, "No undo needed")
        );
        let y = &s.days[1].rows[0];
        assert_eq!(y.subtitle, "12:00 · 7.7 GB · from Storage");
        assert_eq!((y.icon, y.hint.as_str()), (Icon::Trash, "Restore"));
        assert_eq!(s.detail.as_ref().unwrap().id, "c");
    }

    #[test]
    fn a_picked_record_is_shown_and_an_unknown_id_falls_back_to_the_newest() {
        let a = clean_up("a", NOW - 3600);
        let c = clean_up("c", NOW - 60);
        let s = screen(vec![a.clone(), c.clone()], Some("a"), NOW, 0);
        assert_eq!(s.detail.unwrap().id, "a");
        assert!(
            s.days[0]
                .rows
                .iter()
                .find(|r| r.id == "a")
                .unwrap()
                .selected
        );
        let s = screen(vec![a, c], Some("nope"), NOW, 0);
        assert_eq!(s.detail.unwrap().id, "c");
    }

    #[test]
    fn a_clean_up_lists_each_group_and_says_nothing_can_be_undone() {
        let s = screen(vec![clean_up("a", NOW - 60)], None, NOW, 0);
        let d = s.detail.unwrap();
        assert_eq!(d.title, "Cleaned 10.1 GB");
        assert_eq!(d.result, "Partly done");
        assert_eq!(d.when, "Today, 11:59");
        assert_eq!(
            d.lines,
            [
                ("App caches".to_string(), "3.1 GB".to_string()),
                ("Old system logs".to_string(), "skipped".to_string()),
                ("Trash".to_string(), "7.0 GB".to_string()),
            ]
        );
        assert!(d.note.contains("cannot be undone"), "{}", d.note);
        assert_eq!(d.undo, None);
        assert!(d.log.contains("a.json"), "{}", d.log);
    }

    #[test]
    fn items_left_behind_are_named_in_the_note_and_the_result() {
        let mut r = clean_up("a", NOW - 60);
        r.steps[0].status = Status::Partial;
        r.steps[0].entries = vec![Item {
            path: "/home/u/.cache/dotslash".into(),
            bytes: 0,
            result: ItemResult::Left,
            reason: Some("read-only folder: ~/.cache/dotslash".into()),
            trash_name: None,
        }];
        let d = screen(vec![r], None, NOW, 0).detail.unwrap();
        assert_eq!(d.result, "Partly done");
        assert!(d.note.contains("could not be removed"), "{}", d.note);
        assert!(
            d.log
                .contains("left /home/u/.cache/dotslash: read-only folder: ~/.cache/dotslash"),
            "{}",
            d.log
        );
    }

    #[test]
    fn a_trash_move_lists_its_files_and_offers_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let d = screen(vec![trash("b", NOW - 3600, tmp.path())], None, NOW, 0)
            .detail
            .unwrap();
        assert_eq!(d.title, "Moved 2 files to Trash");
        assert_eq!(d.result, "Done");
        assert_eq!(
            d.lines,
            [
                ("ubuntu.iso".to_string(), "6.1 GB".to_string()),
                ("old.zip".to_string(), "1.6 GB".to_string()),
            ]
        );
        assert_eq!(d.note, "The space is only freed after you empty the Trash.");
        assert_eq!(d.undo.as_deref(), Some("Restore from Trash"));
    }

    #[test]
    fn an_ended_app_says_how_many_processes_and_whether_it_was_forced() {
        let normal = history::end_task_record("Firefox", 14, 2_300_000_000, false, NOW - 60, "n");
        let forced = history::end_task_record("Kdenlive", 1, 0, true, NOW - 120, "f");
        let d = screen(vec![normal, forced], Some("f"), NOW, 0)
            .detail
            .unwrap();
        assert_eq!(d.result, "Forced");
        assert_eq!(
            d.lines,
            [
                ("Processes ended".to_string(), "1".to_string()),
                ("Memory freed".to_string(), "0 B".to_string()),
            ]
        );
        assert!(d.note.contains("cannot be undone"));
        assert_eq!(d.undo, None);
    }

    #[test]
    fn an_uninstall_says_where_the_settings_went() {
        let mut kept = step("GIMP", Status::Done, 412_000_000);
        kept.note = None;
        let r = rec(
            "u",
            NOW - 60,
            Kind::Uninstall,
            "Uninstalled GIMP",
            vec![kept],
        );
        let d = screen(vec![r], None, NOW, 0).detail.unwrap();
        assert!(d
            .lines
            .contains(&("GIMP".to_string(), "412 MB".to_string())));
        assert!(d
            .lines
            .contains(&("Your settings & data".to_string(), "kept".to_string())));
        let mut gone = step("GIMP", Status::Done, 592_000_000);
        gone.note = Some("settings and data moved to the Trash".into());
        let r = rec(
            "v",
            NOW - 60,
            Kind::Uninstall,
            "Uninstalled GIMP",
            vec![gone],
        );
        let d = screen(vec![r], None, NOW, 0).detail.unwrap();
        assert!(d.lines.contains(&(
            "Your settings & data".to_string(),
            "in the Trash".to_string()
        )));
    }
}
