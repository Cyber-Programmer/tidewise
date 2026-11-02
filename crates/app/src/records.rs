//! Writing History records for actions the app took. A failure to write is never fatal: the action
//! already happened, so the caller only gets a plain message to show.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use tidewise_core::history::{self, Kind, Record};

/// Seconds since 1970 UTC, 0 if the clock is before that.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Write the record `build(time, id)` makes into `dir`, under a fresh id for `kind`.
pub fn save_in(
    _dir: &Path,
    _kind: Kind,
    _time: u64,
    _build: impl FnOnce(u64, &str) -> Record,
) -> Result<String, String> {
    Err("not written".into())
}

/// Like `save_in`, in the user's History folder, at the current time.
pub fn save(kind: Kind, build: impl FnOnce(u64, &str) -> Record) -> Result<String, String> {
    let dir = history::history_dir().ok_or("no home folder")?;
    save_in(&dir, kind, now_secs(), build)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_is_written_under_a_fresh_id_and_nothing_is_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("state/history");
        let make =
            |time: u64, id: &str| history::end_task_record("Firefox", 3, 10, false, time, id);
        let first = save_in(&dir, Kind::EndTask, 100, make).unwrap();
        let second = save_in(&dir, Kind::EndTask, 100, make).unwrap();
        assert_eq!(first, "100-end_task");
        assert_eq!(second, "100-end_task-2", "same second, same kind: a new id");
        let all = history::list(&dir);
        assert_eq!(all.len(), 2);
        assert!(all
            .iter()
            .all(|r| r.title == "Ended Firefox" && r.time == 100));
    }

    #[test]
    fn an_unwritable_folder_is_a_plain_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a-file");
        std::fs::write(&file, b"x").unwrap();
        let err = save_in(&file.join("history"), Kind::Trash, 5, |t, id| {
            history::end_task_record("X", 1, 0, false, t, id)
        })
        .unwrap_err();
        assert!(!err.is_empty());
    }
}
