//! One JSON file per action the user took (clean up, uninstall, Trash, startup change), kept in
//! `$XDG_STATE_HOME/tidewise/history/`. The History screen reads them; Undo uses what they record.
//! Nothing here leaves the machine.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bumped when a field changes meaning. Readers ignore fields they do not know.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    CleanUp,
    Uninstall,
    Trash,
    Startup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Done,
    /// Needed admin access (or the user closed the prompt); nothing was changed.
    Skipped,
    Failed,
    /// Never started because the user stopped early.
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub title: String,
    pub status: Status,
    pub bytes: u64,
    /// Exactly what was removed or changed (paths or names), for "Copy for a help forum".
    pub items: Vec<String>,
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
                    note: None,
                },
                Step {
                    title: "Old system logs".into(),
                    status: Status::Skipped,
                    bytes: 0,
                    items: vec![],
                    note: Some("needs admin access".into()),
                },
            ],
        }
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
            "\"version\": 1",
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
