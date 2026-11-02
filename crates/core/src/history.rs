//! One JSON file per action the user took (clean up, uninstall, Trash, startup change), kept in
//! `$XDG_STATE_HOME/tidewise/history/`. The History screen reads them; Undo uses what they record.
//! Nothing here leaves the machine.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bumped when a field changes meaning. Readers ignore fields they do not know.
pub const VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    CleanUp,
    Uninstall,
    Trash,
    Startup,
    EndTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Done,
    /// Some items were removed and some were left (could not be removed, or changed since the scan).
    Partial,
    /// Needed admin access (or the user closed the prompt); nothing was changed.
    Skipped,
    Failed,
    /// Never started because the user stopped early.
    NotRun,
}

/// What happened to one item of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemResult {
    Removed,
    Left,
}

/// One item of a step (record version 2): the full path, the bytes that really went away, and why
/// it was left when it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub path: String,
    pub bytes: u64,
    pub result: ItemResult,
    pub reason: Option<String>,
    /// For something moved to the Trash: its name inside the Trash (`files/<name>`), so Undo can
    /// put it back later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trash_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub title: String,
    pub status: Status,
    pub bytes: u64,
    /// Exactly what was removed or changed (paths or names), for "Copy for a help forum".
    pub items: Vec<String>,
    /// Version 2: every item with its result. Empty in version 1 records.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<Item>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub version: u32,
    pub id: String,
    /// Seconds since 1970-01-01 UTC.
    pub time: u64,
    pub kind: Kind,
    pub title: String,
    pub freed_bytes: u64,
    pub steps: Vec<Step>,
}

/// `$XDG_STATE_HOME/tidewise/history`, else `$HOME/.local/state/tidewise/history`.
pub fn history_dir_from(xdg_state: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let state = xdg_state
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|h| h.join(".local/state"))
        })?;
    Some(state.join("tidewise/history"))
}

/// The history folder for this user, from the environment.
pub fn history_dir() -> Option<PathBuf> {
    history_dir_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::CleanUp => "clean_up",
        Kind::Uninstall => "uninstall",
        Kind::Trash => "trash",
        Kind::Startup => "startup",
        Kind::EndTask => "end_task",
    }
}

/// An id that is not used in `dir` yet: `<time>-<kind>`, then `-2`, `-3` ….
pub fn unique_id(dir: &Path, time: u64, kind: Kind) -> String {
    let base = format!("{time}-{}", kind_name(kind));
    let mut id = base.clone();
    let mut n = 2;
    while dir.join(format!("{id}.json")).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// A plain file name: not empty, no separators, no NUL, not hidden, not `.`/`..`.
fn is_plain_name(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('.') && !id.contains(['/', '\\', '\0'])
}

/// Write `<id>.json` atomically (temp file, then rename). Refuses an id that is not a plain file
/// name, and never overwrites an existing record.
pub fn write(dir: &Path, record: &Record) -> io::Result<PathBuf> {
    use std::io::Write;
    if !is_plain_name(&record.id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "record id is not a plain file name",
        ));
    }
    std::fs::create_dir_all(dir)?;
    let target = dir.join(format!("{}.json", record.id));
    let temp = dir.join(format!(".{}.tmp", record.id));
    let json = serde_json::to_string_pretty(record).map_err(io::Error::other)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = file
        .write_all(json.as_bytes())
        .and_then(|()| file.sync_all())
        // `hard_link` fails when the target exists, unlike `rename`, so nothing is overwritten.
        .and_then(|()| std::fs::hard_link(&temp, &target));
    let _ = std::fs::remove_file(&temp);
    result?;
    Ok(target)
}

/// All readable records, newest first. Files that are not records are skipped.
pub fn list(dir: &Path) -> Vec<Record> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut records: Vec<Record> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|text| serde_json::from_str(&text).ok())
        .collect();
    records.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.id.cmp(&b.id)));
    records
}

/// Records of one day, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayGroup {
    pub label: String,
    pub records: Vec<Record>,
}

/// `Today`, `Yesterday`, `Last week` (2 to 7 days ago), else `October 2026`; in the local day given
/// by `offset_secs` (seconds east of UTC).
pub fn day_label(_time: u64, _now: u64, _offset_secs: i64) -> String {
    String::new()
}

/// `19:41` for today and yesterday, `Oct 21, 14:02` for older ones.
pub fn time_label(_time: u64, _now: u64, _offset_secs: i64) -> String {
    String::new()
}

/// Newest first, one group per label, in order.
pub fn group_by_day(_records: Vec<Record>, _now: u64, _offset_secs: i64) -> Vec<DayGroup> {
    Vec::new()
}

/// A plain-text report to paste when asking for help: the system line, what ran and what happened.
/// Never file contents; the home folder and any `/home/<name>` are written as `~`.
pub fn report_text(_record: &Record, _system: &str) -> String {
    String::new()
}

/// The record of a Storage "Move to Trash": each file's original place, its name in the Trash and
/// its size (0 when unknown). Nothing is freed until the Trash is emptied, so `freed_bytes` is 0.
pub fn trash_record(_moved: &[(crate::trash::Trashed, u64)], _time: u64, _id: &str) -> Record {
    Record {
        version: VERSION,
        id: _id.into(),
        time: _time,
        kind: Kind::Trash,
        title: String::new(),
        freed_bytes: 0,
        steps: Vec::new(),
    }
}

/// The record of an ended app: how many processes, the memory it gave back, and whether it had to
/// be force quit.
pub fn end_task_record(
    _name: &str,
    _processes: usize,
    _freed: u64,
    _forced: bool,
    _time: u64,
    _id: &str,
) -> Record {
    Record {
        version: VERSION,
        id: _id.into(),
        time: _time,
        kind: Kind::EndTask,
        title: String::new(),
        freed_bytes: 0,
        steps: Vec::new(),
    }
}

/// What Undo would do for this record without admin access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Undo {
    RestoreFromTrash { count: usize },
}

pub fn undo_kind(_record: &Record) -> Option<Undo> {
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UndoReport {
    pub restored: usize,
    /// File name and a plain reason, for what could not be put back.
    pub failed: Vec<(String, String)>,
}

/// Put back what the record moved to the Trash at `trash_dir`. Names that are not a plain file name
/// are refused; nothing is overwritten.
pub fn undo_trash(_record: &Record, _trash_dir: &Path) -> UndoReport {
    UndoReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn record(id: &str, time: u64) -> Record {
        Record {
            version: VERSION,
            id: id.into(),
            time,
            kind: Kind::CleanUp,
            title: "Cleaned 3.1 GB".into(),
            freed_bytes: 3_100_000_000,
            steps: vec![
                Step {
                    title: "App caches".into(),
                    status: Status::Done,
                    bytes: 3_100_000_000,
                    items: vec!["/home/u/.cache/alpha".into()],
                    entries: vec![],
                    note: None,
                },
                Step {
                    title: "Old system logs".into(),
                    status: Status::Skipped,
                    bytes: 0,
                    items: vec![],
                    entries: vec![],
                    note: Some("needs admin access".into()),
                },
            ],
        }
    }

    /// The shape run 06 wrote (taken from a real record, shortened): no `entries`, plain item paths.
    const V1_RECORD: &str = r#"{"version":1,"id":"1793580727-clean_up","time":1793580727,"kind":"clean_up",
        "title":"Cleaned 19.5 GB","freed_bytes":19477192704,"steps":[
        {"title":"App caches","status":"done","bytes":17900896256,
         "items":["/home/u/.cache/mozilla","/home/u/.cache/uv"],
         "note":"dotslash: It could not be removed: Permission denied (os error 13)"}]}"#;

    #[test]
    fn a_version_1_record_still_reads() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("1793580727-clean_up.json"), V1_RECORD).unwrap();
        let all = list(tmp.path());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].version, 1);
        assert_eq!(all[0].steps[0].status, Status::Done);
        assert_eq!(all[0].steps[0].items.len(), 2);
        assert!(all[0].steps[0].entries.is_empty());
    }

    #[test]
    fn a_version_2_step_keeps_each_items_result_and_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let mut r = record("7-clean_up", 7);
        r.steps[0].status = Status::Partial;
        r.steps[0].entries = vec![
            Item {
                path: "/home/u/.cache/alpha".into(),
                bytes: 10,
                result: ItemResult::Removed,
                reason: None,
                trash_name: None,
            },
            Item {
                path: "/home/u/.cache/dotslash".into(),
                bytes: 0,
                result: ItemResult::Left,
                reason: Some("read-only folder: ~/.cache/dotslash".into()),
                trash_name: None,
            },
        ];
        write(tmp.path(), &r).unwrap();
        let text = fs::read_to_string(tmp.path().join("7-clean_up.json")).unwrap();
        for needle in [
            "\"status\": \"partial\"",
            "\"result\": \"left\"",
            "\"reason\": \"read-only folder: ~/.cache/dotslash\"",
        ] {
            assert!(text.contains(needle), "missing {needle} in {text}");
        }
        assert_eq!(list(tmp.path()), vec![r]);
    }

    // ---- Q07: the History screen's data -----------------------------------------------------

    /// 2026-11-02 12:00:00 UTC.
    const NOW: u64 = 1_793_620_800;
    const DAY: u64 = 86_400;
    const PKT: i64 = 5 * 3600;

    #[test]
    fn days_are_labelled_in_the_local_day() {
        assert_eq!(day_label(NOW - 3600, NOW, 0), "Today");
        assert_eq!(day_label(NOW - DAY, NOW, 0), "Yesterday");
        assert_eq!(day_label(NOW - 3 * DAY, NOW, 0), "Last week");
        assert_eq!(day_label(NOW - 7 * DAY, NOW, 0), "Last week");
        assert_eq!(day_label(NOW - 20 * DAY, NOW, 0), "October 2026");
        assert_eq!(day_label(NOW - 400 * DAY, NOW, 0), "September 2025");
        // 2026-11-01 21:00 UTC is already 02:00 on 2 Nov in Pakistan: today there, yesterday in UTC.
        let t = NOW - 15 * 3600;
        assert_eq!(day_label(t, NOW, 0), "Yesterday");
        assert_eq!(day_label(t, NOW, PKT), "Today");
    }

    #[test]
    fn times_are_short_for_two_days_and_dated_after() {
        assert_eq!(time_label(NOW - 3600, NOW, 0), "11:00");
        assert_eq!(time_label(NOW - DAY, NOW, PKT), "17:00");
        // 2026-10-21 14:02:00 UTC
        assert_eq!(time_label(1_792_591_320, NOW, 0), "Oct 21, 14:02");
    }

    #[test]
    fn records_group_by_day_newest_first() {
        let mut older = record("a", NOW - 20 * DAY);
        older.title = "old".into();
        let groups = group_by_day(
            vec![
                record("b", NOW - DAY),
                older,
                record("c", NOW - 60),
                record("d", NOW - 2 * 3600),
            ],
            NOW,
            0,
        );
        let got: Vec<_> = groups
            .iter()
            .map(|g| {
                (
                    g.label.as_str(),
                    g.records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("Today", vec!["c", "d"]),
                ("Yesterday", vec!["b"]),
                ("October 2026", vec!["a"])
            ]
        );
    }

    #[test]
    fn the_forum_report_names_the_system_and_what_ran_and_hides_the_home_folder() {
        let mut r = record("1-clean_up", NOW - 600);
        r.steps[0].items = vec![
            "/home/bob/.cache/alpha".into(),
            "/home/bob/.cache/beta".into(),
        ];
        r.steps[0].entries = vec![Item {
            path: "/home/bob/.cache/dotslash".into(),
            bytes: 0,
            result: ItemResult::Left,
            reason: Some("read-only folder: ~/.cache/dotslash".into()),
            trash_name: None,
        }];
        let text = report_text(&r, "Arch Linux · KDE Plasma 6 · pacman");
        assert!(text.contains("Cleaned 3.1 GB"), "{text}");
        assert!(
            text.contains("System: Arch Linux · KDE Plasma 6 · pacman"),
            "{text}"
        );
        assert!(
            text.contains("App caches") && text.contains("Old system logs"),
            "{text}"
        );
        assert!(
            text.contains("~/.cache/alpha") && text.contains("~/.cache/dotslash"),
            "{text}"
        );
        assert!(!text.contains("bob") && !text.contains("/home/"), "{text}");
        assert!(text.contains("2026-11-02"), "a date: {text}");
    }

    fn two_trashed(
        tmp: &std::path::Path,
    ) -> (std::path::PathBuf, Vec<(crate::trash::Trashed, u64)>) {
        let home = tmp.join("home");
        let trash = home.join(".local/share/Trash");
        std::fs::create_dir_all(home.join("Downloads")).unwrap();
        let mut moved = Vec::new();
        for (name, size) in [("a.iso", 5000u64), ("b.zip", 700)] {
            let path = home.join("Downloads").join(name);
            std::fs::write(&path, vec![1u8; size as usize]).unwrap();
            let t = crate::trash::move_to_trash(
                &path,
                &trash,
                &home,
                std::time::SystemTime::UNIX_EPOCH,
            )
            .unwrap();
            moved.push((t, size));
        }
        (trash, moved)
    }

    #[test]
    fn a_storage_trash_action_is_recorded_with_where_each_file_went() {
        let tmp = tempfile::tempdir().unwrap();
        let (_trash, moved) = two_trashed(tmp.path());
        let r = trash_record(&moved, NOW, "9-trash");
        assert_eq!((r.kind, r.freed_bytes), (Kind::Trash, 0));
        assert_eq!(r.title, "Moved 2 files to Trash");
        assert_eq!(r.steps.len(), 1);
        assert_eq!(r.steps[0].status, Status::Done);
        assert_eq!(r.steps[0].bytes, 5700);
        assert_eq!(r.steps[0].entries.len(), 2);
        let e = &r.steps[0].entries[0];
        assert!(e.path.ends_with("Downloads/a.iso"), "{}", e.path);
        assert_eq!((e.bytes, e.result), (5000, ItemResult::Removed));
        assert!(e
            .trash_name
            .as_deref()
            .is_some_and(|n| n.starts_with("a.iso")));
        let one = trash_record(&moved[..1], NOW, "9-trash");
        assert_eq!(one.title, "Moved 1 file to Trash");
    }

    #[test]
    fn an_ended_app_is_recorded_with_its_processes_and_memory() {
        let r = end_task_record("Firefox", 14, 2_300_000_000, false, NOW, "5-end_task");
        assert_eq!((r.kind, r.title.as_str()), (Kind::EndTask, "Ended Firefox"));
        assert_eq!(r.freed_bytes, 2_300_000_000);
        assert_eq!(r.steps[0].status, Status::Done);
        assert_eq!(r.steps[0].note.as_deref(), Some("14 processes ended"));
        let forced = end_task_record("Kdenlive", 1, 0, true, NOW, "6-end_task");
        assert_eq!(
            forced.steps[0].note.as_deref(),
            Some("1 process ended · force quit")
        );
    }

    #[test]
    fn only_trash_moves_can_be_undone_without_admin() {
        let tmp = tempfile::tempdir().unwrap();
        let (_trash, moved) = two_trashed(tmp.path());
        assert_eq!(
            undo_kind(&trash_record(&moved, NOW, "9-trash")),
            Some(Undo::RestoreFromTrash { count: 2 })
        );
        assert_eq!(undo_kind(&record("1-clean_up", NOW)), None);
        assert_eq!(
            undo_kind(&end_task_record("X", 1, 0, false, NOW, "1")),
            None
        );
    }

    #[test]
    fn undo_puts_the_files_back_once_and_never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let (trash, moved) = two_trashed(tmp.path());
        let r = trash_record(&moved, NOW, "9-trash");
        let report = undo_trash(&r, &trash);
        assert_eq!((report.restored, report.failed.len()), (2, 0), "{report:?}");
        assert!(moved.iter().all(|(t, _)| t.original.exists()));
        // Again: the Trash no longer has them.
        let again = undo_trash(&r, &trash);
        assert_eq!(again.restored, 0);
        assert_eq!(again.failed.len(), 2);
        // Something new in the old place is never overwritten.
        let (trash2, moved2) = two_trashed(&tmp.path().join("second"));
        std::fs::write(&moved2[0].0.original, b"new").unwrap();
        let r2 = trash_record(&moved2, NOW, "8-trash");
        let report = undo_trash(&r2, &trash2);
        assert_eq!(report.restored, 1);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(std::fs::read(&moved2[0].0.original).unwrap(), b"new");
    }

    #[test]
    fn undo_refuses_names_that_could_leave_the_trash() {
        let tmp = tempfile::tempdir().unwrap();
        let (trash, moved) = two_trashed(tmp.path());
        let victim = tmp.path().join("home/important.txt");
        std::fs::write(&victim, b"keep").unwrap();
        let mut r = trash_record(&moved, NOW, "9-trash");
        for (i, bad) in ["../../important.txt", "a/b", "", ".", "..", "x\0y"]
            .iter()
            .enumerate()
        {
            r.steps[0].entries[i % 2].trash_name = Some((*bad).to_string());
            let report = undo_trash(&r, &trash);
            assert_eq!(report.restored, 0, "{bad:?}");
        }
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
    }

    #[test]
    fn history_lives_under_the_state_folder() {
        let dir = |x: Option<&str>, h: Option<&str>| {
            history_dir_from(x.map(Into::into), h.map(Into::into))
        };
        assert_eq!(
            dir(Some("/s"), Some("/h")),
            Some("/s/tidewise/history".into())
        );
        assert_eq!(
            dir(None, Some("/h")),
            Some("/h/.local/state/tidewise/history".into())
        );
        assert_eq!(
            dir(Some("relative"), Some("/h")),
            Some("/h/.local/state/tidewise/history".into())
        );
        assert_eq!(dir(Some(""), None), None);
        assert_eq!(dir(None, None), None);
    }

    #[test]
    fn a_record_survives_write_and_list() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("state/history");
        let r = record("100-clean_up", 100);
        let path = write(&dir, &r).unwrap();
        assert_eq!(path, dir.join("100-clean_up.json"));
        assert_eq!(list(&dir), vec![r]);
        let leftovers: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(leftovers.len(), 1, "no temp file stays behind");
    }

    #[test]
    fn json_has_the_documented_shape() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), &record("1-clean_up", 1)).unwrap();
        let text = fs::read_to_string(tmp.path().join("1-clean_up.json")).unwrap();
        for needle in [
            "\"version\": 2",
            "\"kind\": \"clean_up\"",
            "\"status\": \"skipped\"",
            "\"freed_bytes\": 3100000000",
        ] {
            assert!(text.contains(needle), "missing {needle} in {text}");
        }
    }

    #[test]
    fn ids_that_could_leave_the_folder_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        for bad in [
            "",
            "../x",
            "a/b",
            "/etc/passwd",
            ".hidden",
            "a\0b",
            "..",
            "x.json/..",
        ] {
            assert!(write(tmp.path(), &record(bad, 1)).is_err(), "{bad:?}");
        }
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn an_existing_record_is_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), &record("5-clean_up", 5)).unwrap();
        let mut other = record("5-clean_up", 5);
        other.title = "different".into();
        assert!(write(tmp.path(), &other).is_err());
        assert_eq!(list(tmp.path())[0].title, "Cleaned 3.1 GB");
    }

    #[test]
    fn unique_ids_count_up() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(unique_id(tmp.path(), 7, Kind::Trash), "7-trash");
        write(tmp.path(), &record("7-trash", 7)).unwrap();
        assert_eq!(unique_id(tmp.path(), 7, Kind::Trash), "7-trash-2");
        write(tmp.path(), &record("7-trash-2", 7)).unwrap();
        assert_eq!(unique_id(tmp.path(), 7, Kind::Trash), "7-trash-3");
    }

    #[test]
    fn list_is_newest_first_and_skips_what_is_not_a_record() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), &record("a", 10)).unwrap();
        write(tmp.path(), &record("b", 30)).unwrap();
        write(tmp.path(), &record("c", 20)).unwrap();
        fs::write(tmp.path().join("broken.json"), "{ not json").unwrap();
        fs::write(tmp.path().join("notes.txt"), "hello").unwrap();
        fs::create_dir(tmp.path().join("sub.json")).unwrap();
        let ids: Vec<String> = list(tmp.path()).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["b", "c", "a"]);
    }

    #[test]
    fn a_missing_folder_lists_nothing() {
        assert_eq!(list(Path::new("/nonexistent/tidewise/history")), vec![]);
    }

    #[test]
    fn newer_files_with_extra_fields_still_read() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("x.json"),
            r#"{"version":2,"id":"x","time":9,"kind":"uninstall","title":"t","freed_bytes":1,"steps":[],"undo":{"later":true}}"#,
        )
        .unwrap();
        assert_eq!(list(tmp.path())[0].kind, Kind::Uninstall);
    }
}
