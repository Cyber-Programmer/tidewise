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

fn icon(kind: Kind) -> Icon {
    match kind {
        Kind::CleanUp => Icon::Clean,
        Kind::Uninstall => Icon::App,
        Kind::Trash => Icon::Trash,
        Kind::Startup => Icon::Startup,
        Kind::EndTask => Icon::Process,
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn count(record: &Record, status: Status) -> usize {
    record.steps.iter().filter(|s| s.status == status).count()
}

fn left_behind(record: &Record) -> usize {
    record
        .steps
        .iter()
        .flat_map(|s| &s.entries)
        .filter(|e| e.result == history::ItemResult::Left)
        .count()
}

fn forced(record: &Record) -> bool {
    record.kind == Kind::EndTask
        && record
            .steps
            .iter()
            .any(|s| s.note.as_deref().is_some_and(|n| n.contains("force quit")))
}

/// One short line under the title.
fn summary(record: &Record) -> String {
    match record.kind {
        Kind::CleanUp => {
            let cleaned = count(record, Status::Done) + count(record, Status::Partial);
            let mut text = if cleaned == 0 {
                "nothing was removed".to_string()
            } else {
                format!("{} cleaned", plural(cleaned, "group", "groups"))
            };
            let skipped = count(record, Status::Skipped);
            if skipped > 0 {
                text += &format!(" · {skipped} skipped");
            }
            let left = left_behind(record);
            if left > 0 {
                text += &format!(" · {left} left");
            }
            text
        }
        Kind::Uninstall => {
            if record.freed_bytes > 0 {
                format!("{} freed", history::bytes_text(record.freed_bytes))
            } else {
                String::new()
            }
        }
        Kind::Trash => {
            let bytes: u64 = record.steps.iter().map(|s| s.bytes).sum();
            if bytes > 0 {
                format!("{} · from Storage", history::bytes_text(bytes))
            } else {
                "from Storage".into()
            }
        }
        Kind::EndTask => record
            .steps
            .first()
            .and_then(|s| s.note.clone())
            .unwrap_or_default(),
        Kind::Startup => record
            .steps
            .first()
            .map(|s| s.title.clone())
            .unwrap_or_default(),
    }
}

fn result(record: &Record) -> &'static str {
    if forced(record) {
        return "Forced";
    }
    let (done, partial, failed, skipped) = (
        count(record, Status::Done),
        count(record, Status::Partial),
        count(record, Status::Failed),
        count(record, Status::Skipped),
    );
    if failed > 0 && done + partial == 0 {
        "Failed"
    } else if (partial + failed + skipped > 0) && (done + partial > 0) {
        "Partly done"
    } else if skipped > 0 {
        "Skipped"
    } else {
        "Done"
    }
}

fn word(status: Status) -> &'static str {
    match status {
        Status::Done => "done",
        Status::Partial => "partly done",
        Status::Skipped => "skipped",
        Status::Failed => "failed",
        Status::NotRun => "not run",
    }
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// What changed: label and value.
fn lines(record: &Record) -> Vec<(String, String)> {
    let size = |bytes: u64| {
        if bytes > 0 {
            history::bytes_text(bytes)
        } else {
            "—".to_string()
        }
    };
    match record.kind {
        Kind::Trash => record
            .steps
            .iter()
            .flat_map(|s| &s.entries)
            .map(|e| (file_name(&e.path), size(e.bytes)))
            .collect(),
        Kind::EndTask => {
            let n = record
                .steps
                .first()
                .and_then(|s| s.note.as_deref())
                .and_then(|n| n.split_whitespace().next())
                .unwrap_or("1")
                .to_string();
            vec![
                ("Processes ended".into(), n),
                (
                    "Memory freed".into(),
                    history::bytes_text(record.freed_bytes),
                ),
            ]
        }
        Kind::Uninstall => {
            let mut out: Vec<(String, String)> = record
                .steps
                .iter()
                .map(|s| (s.title.clone(), history::bytes_text(s.bytes)))
                .collect();
            let notes: Vec<&str> = record
                .steps
                .iter()
                .filter_map(|s| s.note.as_deref())
                .collect();
            if !notes.contains(&"moved to the Trash") {
                let value = if notes.iter().any(|n| n.contains("settings and data moved")) {
                    "in the Trash"
                } else {
                    "kept"
                };
                out.push(("Your settings & data".into(), value.into()));
            }
            out
        }
        Kind::Startup => record
            .steps
            .iter()
            .map(|s| {
                (
                    s.title.clone(),
                    s.note.clone().unwrap_or_else(|| "done".into()),
                )
            })
            .collect(),
        _ => record
            .steps
            .iter()
            .map(|s| {
                let value = if s.bytes > 0 {
                    history::bytes_text(s.bytes)
                } else {
                    word(s.status).to_string()
                };
                (s.title.clone(), value)
            })
            .collect(),
    }
}

fn note(record: &Record) -> String {
    let notes: Vec<&str> = record
        .steps
        .iter()
        .filter_map(|s| s.note.as_deref())
        .collect();
    match record.kind {
        Kind::CleanUp if left_behind(record) > 0 || count(record, Status::Partial) > 0 => {
            "Some items could not be removed, so they are still there. The technical details list each one with the reason.".into()
        }
        Kind::CleanUp => {
            "Cleaning cannot be undone, but caches and logs rebuild on their own.".into()
        }
        Kind::Uninstall if notes.contains(&"moved to the Trash") => {
            "The files are in your Trash, so you can still put them back from there.".into()
        }
        Kind::Uninstall if notes.iter().any(|n| n.contains("settings and data moved")) => {
            "Your settings and data are in your Trash, so you can still put them back from there."
                .into()
        }
        Kind::Uninstall => {
            "Your settings were kept, so reinstalling later brings everything back.".into()
        }
        Kind::Trash => "The space is only freed after you empty the Trash.".into(),
        Kind::EndTask => "Ending an app cannot be undone.".into(),
        Kind::Startup => {
            "The app is still installed. Open it from your app menu whenever you like.".into()
        }
    }
}

/// The technical details: every path with what happened to it, and where the record is saved.
fn log(record: &Record) -> String {
    let mut out = Vec::new();
    for step in &record.steps {
        for item in step
            .items
            .iter()
            .filter(|i| !step.entries.iter().any(|e| &e.path == *i))
        {
            out.push(if record.kind == Kind::Startup {
                format!("wrote {item}")
            } else {
                format!("removed {item}")
            });
        }
        for e in &step.entries {
            out.push(match (&e.result, &e.reason) {
                (history::ItemResult::Removed, _) => format!("removed {}", e.path),
                (history::ItemResult::Left, Some(why)) => format!("left {}: {why}", e.path),
                (history::ItemResult::Left, None) => format!("left {}", e.path),
            });
        }
    }
    out.push(format!(
        "saved: ~/.local/state/tidewise/history/{}.json",
        record.id
    ));
    out.join("\n")
}

fn row(record: &Record, now: u64, offset: i64, selected: bool) -> Row {
    let hint = if history::undo_kind(record).is_some() {
        "Restore"
    } else if record.kind == Kind::CleanUp {
        "No undo needed"
    } else {
        ""
    };
    Row {
        id: record.id.clone(),
        title: record.title.clone(),
        subtitle: format!(
            "{} · {}",
            history::time_label(record.time, now, offset),
            summary(record)
        )
        .trim_end_matches(" · ")
        .to_string(),
        icon: icon(record.kind),
        hint: hint.into(),
        selected,
    }
}

fn detail(record: &Record, now: u64, offset: i64) -> Detail {
    Detail {
        id: record.id.clone(),
        title: record.title.clone(),
        result: result(record).into(),
        when: format!(
            "{}, {}",
            history::day_label(record.time, now, offset),
            history::time_label(record.time, now, offset)
        ),
        lines: lines(record),
        note: note(record),
        log: log(record),
        undo: history::undo_kind(record).map(|u| match u {
            Undo::RestoreFromTrash { .. } => "Restore from Trash".to_string(),
        }),
    }
}

/// Build the screen. `selected` is a record id; when it is unknown the newest record is shown.
pub fn screen(records: Vec<Record>, selected: Option<&str>, now: u64, offset_secs: i64) -> Screen {
    let groups = history::group_by_day(records, now, offset_secs);
    let newest = groups
        .first()
        .and_then(|g| g.records.first())
        .map(|r| r.id.clone());
    let picked = selected
        .filter(|id| groups.iter().any(|g| g.records.iter().any(|r| r.id == *id)))
        .map(str::to_string)
        .or(newest);
    let days = groups
        .iter()
        .map(|g| Day {
            label: g.label.clone(),
            rows: g
                .records
                .iter()
                .map(|r| {
                    row(
                        r,
                        now,
                        offset_secs,
                        picked.as_deref() == Some(r.id.as_str()),
                    )
                })
                .collect(),
        })
        .collect();
    let detail = picked.as_deref().and_then(|id| {
        groups
            .iter()
            .flat_map(|g| &g.records)
            .find(|r| r.id == id)
            .map(|r| detail(r, now, offset_secs))
    });
    Screen { days, detail }
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
    fn a_startup_change_shows_before_and_after() {
        let r = history::startup_record(
            "Discord",
            false,
            "/home/u/.config/autostart/discord.desktop",
            NOW - 60,
            "s",
        );
        let s = screen(vec![r], None, NOW, 0);
        assert_eq!(s.days[0].rows[0].subtitle, "11:59 · Discord at login");
        let d = s.detail.unwrap();
        assert_eq!(
            d.lines,
            [("Discord at login".to_string(), "on → off".to_string())]
        );
        assert_eq!(d.result, "Done");
        assert!(d.note.contains("still installed"), "{}", d.note);
        assert!(
            d.log.contains("/home/u/.config/autostart/discord.desktop"),
            "{}",
            d.log
        );
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
