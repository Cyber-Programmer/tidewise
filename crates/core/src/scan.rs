//! Disk scanner: how big is each folder, which files are biggest, which were not opened for a
//! long time. Read-only. Counts every inode once, never follows symlinks and stays on the
//! filesystem of the folder it starts in (like `du -sx`).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

/// Six months, the "not opened in a while" limit.
pub const STALE_AFTER: Duration = Duration::from_secs(60 * 60 * 24 * 183);

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// How many of the biggest files to keep.
    pub biggest: usize,
    /// Files not used for this long count as stale.
    pub stale_after: Duration,
    /// Files below this size are never listed as stale (a tiny old file is not worth a mention).
    pub stale_min_bytes: u64,
    /// The clock used for "not used for"; tests set it.
    pub now: SystemTime,
    /// Use the access time. With `noatime`/`relatime` mounts it can be unreliable, so callers may
    /// turn it off and use the modification time instead (see [`ScanResult::used_modified_time`]).
    pub use_access_time: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            biggest: 50,
            stale_after: STALE_AFTER,
            stale_min_bytes: 1_000_000,
            now: SystemTime::now(),
            use_access_time: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Bytes on disk (allocated blocks), as `du` counts them.
    pub bytes: u64,
    pub last_used: SystemTime,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanResult {
    pub root: PathBuf,
    pub total_bytes: u64,
    /// Direct children of the root with their sizes, biggest first. Files and folders alike.
    pub children: Vec<(PathBuf, u64)>,
    pub biggest_files: Vec<FileEntry>,
    /// Files not used for `stale_after`, biggest first.
    pub stale_files: Vec<FileEntry>,
    /// True when the modification time stood in for the access time.
    pub used_modified_time: bool,
    /// Folders directly inside the root that sit on another file system (mounts). Not counted, not
    /// entered; the caller can still list them.
    pub other_drives: Vec<PathBuf>,
    /// Things that could not be read (permissions, vanished files). Not an error.
    pub skipped: u64,
    /// True when the scan was cancelled and the numbers are partial.
    pub cancelled: bool,
}

/// Live numbers for a progress bar.
#[derive(Debug, Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub bytes: AtomicU64,
}

pub struct ScanHandle {
    pub progress: Arc<Progress>,
    cancel: Arc<AtomicBool>,
    thread: JoinHandle<ScanResult>,
}

impl ScanHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// True once the scan has ended (finished or cancelled); `wait` then returns at once.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Wait for the scan to end (or to stop after `cancel`).
    pub fn wait(self) -> ScanResult {
        self.thread.join().unwrap_or_default()
    }
}

/// Scan `root` on a background thread.
pub fn start(root: PathBuf, options: ScanOptions) -> ScanHandle {
    let progress = Arc::new(Progress::default());
    let cancel = Arc::new(AtomicBool::new(false));
    let thread = {
        let (progress, cancel) = (progress.clone(), cancel.clone());
        std::thread::spawn(move || scan(&root, &options, &progress, &cancel))
    };
    ScanHandle {
        progress,
        cancel,
        thread,
    }
}

struct Walk<'a> {
    options: &'a ScanOptions,
    progress: &'a Progress,
    cancel: &'a AtomicBool,
    device: u64,
    /// (device, inode) of files with several hard links that we already counted in the total.
    seen: HashSet<(u64, u64)>,
    /// The same, but only for the child of the root being walked: a file shared by two children is
    /// in the size of both, so a child's size is what you get when you open it.
    seen_child: HashSet<(u64, u64)>,
    /// Everything counted once: the root's own blocks and every inode once.
    total: u64,
    out: ScanResult,
    stale_cut: SystemTime,
}

/// Scan `root` on this thread.
pub fn scan(
    root: &Path,
    options: &ScanOptions,
    progress: &Progress,
    cancel: &AtomicBool,
) -> ScanResult {
    let mut walk = Walk {
        options,
        progress,
        cancel,
        device: 0,
        seen: HashSet::new(),
        seen_child: HashSet::new(),
        total: 0,
        out: ScanResult {
            root: root.to_path_buf(),
            used_modified_time: !options.use_access_time,
            ..ScanResult::default()
        },
        stale_cut: options
            .now
            .checked_sub(options.stale_after)
            .unwrap_or(SystemTime::UNIX_EPOCH),
    };
    let Ok(meta) = fs::symlink_metadata(root) else {
        walk.out.skipped += 1;
        return walk.out;
    };
    walk.device = meta.dev();
    let mut children: HashMap<PathBuf, u64> = HashMap::new();
    if meta.is_dir() {
        // Like `du -s`, the folder's own blocks are part of its size.
        let own = meta.blocks() * 512;
        walk.progress.bytes.fetch_add(own, Ordering::Relaxed);
        walk.total += own;
        walk.dir(root, root, &mut children);
    } else {
        let (in_child, in_total) = walk.file(root, &meta);
        walk.total += in_total;
        children.insert(root.to_path_buf(), in_child);
    }
    walk.out.total_bytes = walk.total;
    let mut children: Vec<_> = children.into_iter().collect();
    children.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    walk.out.children = children;
    walk.out.cancelled = cancel.load(Ordering::Relaxed);
    walk.out.other_drives.sort();
    walk.finish()
}

impl Walk<'_> {
    fn finish(mut self) -> ScanResult {
        let by_size =
            |a: &FileEntry, b: &FileEntry| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path));
        self.out.biggest_files.sort_by(by_size);
        self.out.biggest_files.truncate(self.options.biggest);
        self.out.stale_files.sort_by(by_size);
        self.out.stale_files.truncate(self.options.biggest);
        self.out
    }

    /// Walk `dir`, adding each direct child of `root` to `children`. Returns nothing: sizes land in
    /// `children` keyed by the child of `root` they are below.
    fn dir(&mut self, root: &Path, dir: &Path, children: &mut HashMap<PathBuf, u64>) {
        let Ok(entries) = fs::read_dir(dir) else {
            self.out.skipped += 1;
            return;
        };
        for entry in entries {
            if self.cancel.load(Ordering::Relaxed) {
                return;
            }
            if dir == root {
                self.seen_child.clear();
            }
            let Ok(entry) = entry else {
                self.out.skipped += 1;
                continue;
            };
            let path = entry.path();
            let Ok(meta) = fs::symlink_metadata(&path) else {
                self.out.skipped += 1;
                continue;
            };
            // The top-level child this entry belongs to.
            let top = path
                .strip_prefix(root)
                .ok()
                .and_then(|p| p.components().next())
                .map(|c| root.join(c))
                .unwrap_or_else(|| path.clone());
            if meta.is_dir() {
                // Other filesystems (mounts, /proc, network shares) are not part of this folder.
                if meta.dev() != self.device {
                    if dir == root {
                        self.out.other_drives.push(path);
                    }
                    continue;
                }
                // A folder's own blocks count in the total and in progress: one rule, same as `du`.
                let bytes = meta.blocks() * 512;
                self.progress.bytes.fetch_add(bytes, Ordering::Relaxed);
                *children.entry(top).or_insert(0) += bytes;
                self.total += bytes;
                self.dir(root, &path, children);
            } else {
                let (in_child, in_total) = self.file(&path, &meta);
                *children.entry(top).or_insert(0) += in_child;
                self.total += in_total;
            }
        }
    }

    /// Count one non-directory (a symlink counts as itself, not its target). Returns the bytes to
    /// add to the child being walked and to the total: a hard link already seen counts 0 in the
    /// total, and 0 in the child only if that child has it already.
    fn file(&mut self, path: &Path, meta: &fs::Metadata) -> (u64, u64) {
        let bytes = meta.blocks() * 512;
        let (in_total, in_child) = if meta.nlink() > 1 {
            let key = (meta.dev(), meta.ino());
            (self.seen.insert(key), self.seen_child.insert(key))
        } else {
            (true, true)
        };
        if !in_total {
            return (if in_child { bytes } else { 0 }, 0);
        }
        self.progress.files.fetch_add(1, Ordering::Relaxed);
        self.progress.bytes.fetch_add(bytes, Ordering::Relaxed);
        if !meta.is_file() {
            return (bytes, bytes);
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let last_used = if self.options.use_access_time {
            meta.accessed().unwrap_or(modified)
        } else {
            modified
        };
        // Reading a file bumps its access time; the newer of the two is the last real use.
        let last_used = last_used.max(modified);
        let entry = || FileEntry {
            path: path.to_path_buf(),
            bytes,
            last_used,
        };
        self.keep_biggest(entry());
        if last_used < self.stale_cut && bytes >= self.options.stale_min_bytes {
            self.out.stale_files.push(entry());
        }
        (bytes, bytes)
    }

    /// Keep memory bounded: hold at most twice the wanted number, trimming now and then.
    fn keep_biggest(&mut self, entry: FileEntry) {
        let list = &mut self.out.biggest_files;
        list.push(entry);
        let limit = self.options.biggest.max(1) * 2;
        if list.len() > limit * 2 {
            list.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
            list.truncate(limit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    use std::process::Command;
    use std::time::SystemTime;

    /// A temp folder on the project's disk. `/tmp` is often tmpfs, where folders use 0 blocks, which
    /// hides bugs in how folder blocks are counted (ext4 and btrfs charge a block per folder).
    fn disk_tempdir() -> tempfile::TempDir {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp");
        fs::create_dir_all(&base).unwrap();
        tempfile::Builder::new().tempdir_in(base).unwrap()
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = File::create(path).unwrap();
        f.write_all(&vec![7u8; bytes]).unwrap();
        f.sync_all().unwrap();
    }

    fn age(path: &Path, days: u64) {
        let then = SystemTime::now() - Duration::from_secs(days * 86_400);
        let f = File::options().write(true).open(path).unwrap();
        f.set_times(fs::FileTimes::new().set_accessed(then).set_modified(then))
            .unwrap();
    }

    fn run(root: &Path, options: &ScanOptions) -> ScanResult {
        scan(root, options, &Progress::default(), &AtomicBool::new(false))
    }

    fn size_of(r: &ScanResult, name: &str) -> u64 {
        r.children
            .iter()
            .find(|(p, _)| p.file_name().unwrap() == name)
            .map_or(0, |c| c.1)
    }

    #[test]
    fn folder_sizes_add_up_and_sort_biggest_first() {
        let dir = disk_tempdir();
        write(&dir.path().join("big/a.bin"), 400_000);
        write(&dir.path().join("big/deep/b.bin"), 300_000);
        write(&dir.path().join("small/c.bin"), 20_000);
        let r = run(dir.path(), &ScanOptions::default());
        assert!((700_000..760_000).contains(&size_of(&r, "big")));
        assert!(size_of(&r, "small") >= 20_000);
        assert_eq!(r.children[0].0.file_name().unwrap(), "big");
        assert!(r.total_bytes >= size_of(&r, "big") + size_of(&r, "small"));
        assert_eq!(r.skipped, 0);
    }

    #[test]
    fn hard_links_are_counted_once() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("one/a.bin"), 500_000);
        fs::create_dir_all(dir.path().join("two")).unwrap();
        fs::hard_link(
            dir.path().join("one/a.bin"),
            dir.path().join("two/a-link.bin"),
        )
        .unwrap();
        let r = run(dir.path(), &ScanOptions::default());
        // The total counts the shared file once; each folder counts it, so opening either shows it.
        assert!(
            (500_000..540_000).contains(&r.total_bytes),
            "{}",
            r.total_bytes
        );
        assert!(size_of(&r, "one") >= 500_000 && size_of(&r, "two") >= 500_000);
    }

    #[test]
    fn symlinks_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(&outside.path().join("huge.bin"), 2_000_000);
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link-to-dir")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("huge.bin"), dir.path().join("link.bin"))
            .unwrap();
        let r = run(dir.path(), &ScanOptions::default());
        assert!(r.total_bytes < 100_000, "{}", r.total_bytes);
        assert!(r.biggest_files.is_empty());
    }

    #[test]
    fn the_biggest_files_are_kept_in_order_and_limited() {
        let dir = tempfile::tempdir().unwrap();
        for (name, size) in [("a", 10_000), ("b", 300_000), ("c", 90_000), ("d", 200_000)] {
            write(&dir.path().join(format!("x/{name}.bin")), size);
        }
        let options = ScanOptions {
            biggest: 2,
            ..ScanOptions::default()
        };
        let names: Vec<_> = run(dir.path(), &options)
            .biggest_files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["b.bin", "d.bin"]);
    }

    #[test]
    fn old_files_are_stale_and_fresh_or_tiny_ones_are_not() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("old.iso"), 3_000_000);
        write(&dir.path().join("new.iso"), 3_000_000);
        write(&dir.path().join("tiny-old.txt"), 100);
        age(&dir.path().join("old.iso"), 400);
        age(&dir.path().join("tiny-old.txt"), 400);
        let r = run(dir.path(), &ScanOptions::default());
        let stale: Vec<_> = r
            .stale_files
            .iter()
            .map(|f| f.path.file_name().unwrap())
            .collect();
        assert_eq!(stale, ["old.iso"]);
        assert!(!r.used_modified_time);
    }

    #[test]
    fn without_access_time_the_modified_time_decides_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.iso"), 3_000_000);
        let then = SystemTime::now() - Duration::from_secs(400 * 86_400);
        let f = File::options()
            .write(true)
            .open(dir.path().join("a.iso"))
            .unwrap();
        // Recently read (access time now) but written long ago.
        f.set_times(
            fs::FileTimes::new()
                .set_accessed(SystemTime::now())
                .set_modified(then),
        )
        .unwrap();
        let by_atime = run(dir.path(), &ScanOptions::default());
        assert!(by_atime.stale_files.is_empty(), "recently opened");
        let by_mtime = run(
            dir.path(),
            &ScanOptions {
                use_access_time: false,
                ..ScanOptions::default()
            },
        );
        assert_eq!(by_mtime.stale_files.len(), 1);
        assert!(by_mtime.used_modified_time);
    }

    #[test]
    fn unreadable_folders_are_skipped_not_fatal() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("ok/a.bin"), 50_000);
        write(&dir.path().join("locked/b.bin"), 50_000);
        let locked = dir.path().join("locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let r = run(dir.path(), &ScanOptions::default());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(size_of(&r, "ok") >= 50_000);
        // Running as root can read it anyway; otherwise it is counted as skipped.
        assert!(r.skipped >= 1 || size_of(&r, "locked") >= 50_000);
    }

    #[test]
    fn a_missing_root_is_an_empty_result() {
        let r = run(Path::new("/definitely/not/here"), &ScanOptions::default());
        assert_eq!((r.total_bytes, r.skipped), (0, 1));
    }

    #[test]
    fn cancelling_stops_the_scan_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..50 {
            write(&dir.path().join(format!("d{i}/f.bin")), 1000);
        }
        let cancel = AtomicBool::new(true);
        let r = scan(
            dir.path(),
            &ScanOptions::default(),
            &Progress::default(),
            &cancel,
        );
        assert!(r.cancelled);
        assert_eq!(r.total_bytes, 0);
    }

    #[test]
    fn the_background_scan_reports_progress_and_returns_the_result() {
        let dir = disk_tempdir();
        write(&dir.path().join("a/f.bin"), 100_000);
        write(&dir.path().join("a/g.bin"), 100_000);
        let handle = start(dir.path().to_path_buf(), ScanOptions::default());
        let progress = handle.progress.clone();
        let r = handle.wait();
        assert_eq!(progress.files.load(Ordering::Relaxed), 2);
        assert_eq!(progress.bytes.load(Ordering::Relaxed), r.total_bytes);
        assert!(!r.cancelled);
    }

    #[test]
    fn progress_and_total_agree_with_nested_folders() {
        let dir = disk_tempdir();
        write(&dir.path().join("a/b/c/f.bin"), 50_000);
        write(&dir.path().join("a/b/g.bin"), 10_000);
        write(&dir.path().join("x/y/h.bin"), 30_000);
        let handle = start(dir.path().to_path_buf(), ScanOptions::default());
        let progress = handle.progress.clone();
        let r = handle.wait();
        assert_eq!(progress.bytes.load(Ordering::Relaxed), r.total_bytes);
    }

    #[test]
    fn a_handle_says_when_the_scan_is_finished() {
        let dir = disk_tempdir();
        write(&dir.path().join("a/f.bin"), 10_000);
        let handle = start(dir.path().to_path_buf(), ScanOptions::default());
        let mut waited = 0;
        while !handle.is_finished() && waited < 500 {
            std::thread::sleep(Duration::from_millis(10));
            waited += 1;
        }
        assert!(handle.is_finished());
        assert!(handle.wait().total_bytes > 0);
    }

    /// Acceptance for M2: our numbers match `du -sx` within 1 % on a test folder.
    #[test]
    fn totals_match_du_within_one_percent() {
        let dir = disk_tempdir();
        for i in 0..20 {
            write(
                &dir.path().join(format!("d{}/f{i}.bin", i % 4)),
                40_000 + i * 7_000,
            );
        }
        let out = Command::new("/usr/bin/du")
            .args(["-sx", "--block-size=1"])
            .arg(dir.path())
            .env_clear()
            .env("LC_ALL", "C")
            .output();
        let Ok(out) = out else { return }; // no du here: nothing to compare with
        let du: f64 = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .expect("du prints a number");
        // `du` also counts the root folder's own block.
        let ours = run(dir.path(), &ScanOptions::default()).total_bytes as f64
            + fs::metadata(dir.path()).unwrap().blocks() as f64 * 512.0;
        assert!((ours - du).abs() / du < 0.01, "ours {ours}, du {du}");
    }

    #[test]
    fn folders_on_another_file_system_are_listed_not_counted() {
        // /dev/shm (tmpfs) and /dev/pts (devpts) are separate file systems inside /dev.
        let r = run(Path::new("/dev"), &ScanOptions::default());
        let shm = Path::new("/dev/shm");
        if shm.is_dir() {
            assert!(
                r.other_drives.contains(&shm.to_path_buf()),
                "{:?}",
                r.other_drives
            );
            assert!(!r.children.iter().any(|(p, _)| p == shm));
        }
        for p in &r.other_drives {
            let (a, b) = (fs::metadata(p).unwrap(), fs::metadata("/dev").unwrap());
            assert_ne!(a.dev(), b.dev(), "{p:?} is on the same file system");
        }
    }

    #[test]
    fn a_normal_tree_has_no_other_drives() {
        let dir = disk_tempdir();
        write(&dir.path().join("a/b.bin"), 10_000);
        assert!(run(dir.path(), &ScanOptions::default())
            .other_drives
            .is_empty());
    }

    /// Folders `a` and `b` share one file by a hard link; `a/deep` has another inside itself.
    fn linked_tree(dir: &Path) {
        write(&dir.join("a/one.bin"), 100_000);
        write(&dir.join("a/deep/two.bin"), 50_000);
        write(&dir.join("b/three.bin"), 80_000);
        write(&dir.join("c/x/y/four.bin"), 30_000);
        fs::hard_link(dir.join("a/one.bin"), dir.join("b/shared.bin")).unwrap();
        fs::hard_link(dir.join("a/deep/two.bin"), dir.join("a/deep/two-again.bin")).unwrap();
    }

    fn check_blocks_equal_opened_totals(dir: &Path) {
        let r = run(dir, &ScanOptions::default());
        for (child, bytes) in r.children.iter().filter(|c| c.0.is_dir()) {
            let opened = run(child, &ScanOptions::default());
            assert_eq!(
                opened.total_bytes,
                *bytes,
                "{} as a block and when opened",
                child.display()
            );
            check_blocks_equal_opened_totals(child);
        }
    }

    #[test]
    fn a_folders_size_is_the_total_shown_when_it_is_opened_at_every_depth() {
        let dir = disk_tempdir();
        linked_tree(dir.path());
        check_blocks_equal_opened_totals(dir.path());
    }

    #[test]
    fn a_link_shared_by_two_folders_is_in_both_but_once_in_the_total() {
        let dir = disk_tempdir();
        linked_tree(dir.path());
        let r = run(dir.path(), &ScanOptions::default());
        let shared = fs::metadata(dir.path().join("a/one.bin")).unwrap().blocks() * 512;
        let own = fs::metadata(dir.path()).unwrap().blocks() * 512;
        let sum: u64 = r.children.iter().map(|c| c.1).sum();
        assert_eq!(
            r.total_bytes + shared,
            sum + own,
            "shared once, the folder's own blocks added"
        );
        let (a, b) = (size_of(&r, "a"), size_of(&r, "b"));
        let b_alone = run(&dir.path().join("b"), &ScanOptions::default());
        assert_eq!(
            b_alone.total_bytes, b,
            "b counts the shared file even though a has it too"
        );
        assert!(a > shared && b > shared);
    }

    #[test]
    fn the_total_counts_the_scanned_folders_own_blocks_like_du() {
        let dir = disk_tempdir();
        write(&dir.path().join("f.bin"), 10_000);
        let r = run(dir.path(), &ScanOptions::default());
        let du = Command::new("du")
            .arg("-sB1")
            .arg(dir.path())
            .output()
            .unwrap();
        let du: u64 = String::from_utf8_lossy(&du.stdout)
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(r.total_bytes, du);
    }
}
