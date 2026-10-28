//! A nested "home folder" on the project's disk, for Storage tests and screenshots.
//!
//! It lives under `target/` (a real disk: `/tmp` is often tmpfs, where folders and files report no
//! blocks) and is removed when the `Fixture` is dropped. Big files are made with `fallocate`, so they
//! really occupy blocks (`du` agrees) without writing gigabytes.
//!
//! ```text
//! home/
//!   Documents/                     ~1.2 GB   (the biggest block)
//!     Archive/disk.img             1.1 GB, shared.bin 40 MB (hard-linked into Games/)
//!     Photos/                      45 files, 100 KB … 3 MB (40+ items in one folder)
//!     Creative Chaos/OpenSource/tidewise/docs/…   (four folders deep)
//!   Games/                         460 MB + the hard link
//!   Downloads/                     two old installers (400 and 300 days)
//!   Music/ Videos/ Notes/ .cache/  110, 110, 70 and 30 MB: the last two group into "Other"; one block comes out short and wide
//!   shortcut -> Documents          (a symlink)
//! ```
#![allow(dead_code)]

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub root: PathBuf,
}

/// `Documents/Creative Chaos/OpenSource/tidewise`, the folder four levels down.
pub const DEEP: &str = "Documents/Creative Chaos/OpenSource/tidewise";

fn disk_tempdir() -> tempfile::TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp");
    fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new().tempdir_in(base).unwrap()
}

/// A file that occupies `bytes` on disk.
fn alloc(path: &Path, bytes: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let done = Command::new("/usr/bin/fallocate")
        .arg("-l")
        .arg(bytes.to_string())
        .arg(path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !done {
        // No fallocate (or a file system without it): a sparse file still has the right length.
        File::create(path).unwrap().set_len(bytes).unwrap();
    }
}

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    File::create(path)
        .unwrap()
        .write_all(&vec![7u8; bytes])
        .unwrap();
}

fn age(path: &Path, days: u64) {
    let then = SystemTime::now() - Duration::from_secs(days * 86_400);
    let f = File::options().write(true).open(path).unwrap();
    f.set_times(fs::FileTimes::new().set_accessed(then).set_modified(then))
        .unwrap();
}

pub fn build() -> Fixture {
    let dir = disk_tempdir();
    let root = dir.path().join("home");
    let p = |rel: &str| root.join(rel);
    const MB: u64 = 1_000_000;

    alloc(&p("Documents/Archive/disk.img"), 1_100 * MB);
    alloc(&p("Documents/Archive/shared.bin"), 40 * MB);
    for i in 0..45usize {
        // 100 KB … ~3 MB, all different, so the order is stable.
        write(
            &p(&format!("Documents/Photos/photo-{i:02}.jpg")),
            100_000 + i * 66_000,
        );
    }
    for (i, name) in ["README.md", "notes.md", "todo.txt", "plan.md"]
        .iter()
        .enumerate()
    {
        write(&p(&format!("{DEEP}/docs/{name}")), 3_000 + i * 700);
    }
    alloc(&p(&format!("{DEEP}/build.bin")), 20 * MB);
    write(&p("Documents/Creative Chaos/OpenSource/LICENSE"), 35_000);

    alloc(&p("Games/Steam/common/game.pak"), 460 * MB);
    fs::hard_link(p("Documents/Archive/shared.bin"), p("Games/shared.bin")).unwrap();

    alloc(&p("Downloads/installer.iso"), 200 * MB);
    age(&p("Downloads/installer.iso"), 400);
    alloc(&p("Downloads/old.zip"), 50 * MB);
    age(&p("Downloads/old.zip"), 300);

    alloc(&p("Music/album/track.flac"), 110 * MB);
    alloc(&p("Videos/clip.mkv"), 110 * MB);
    alloc(&p("Notes/scan.pdf"), 70 * MB);
    alloc(&p(".cache/thumbs.db"), 30 * MB);

    unix_fs::symlink(p("Documents"), p("shortcut")).unwrap();
    Fixture { dir, root }
}
