//! `/proc/mounts`: which filesystem a folder is on and whether its access times can be trusted.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub device: String,
    pub mount_point: String,
    pub fs_type: String,
    pub options: Vec<String>,
}

/// Which timestamp "not opened in 6 months" can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSource {
    AccessTime,
    ModifiedTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessTimeReport {
    pub source: TimeSource,
    /// The mount the folder is on, empty when none was found.
    pub mount_point: String,
    /// Plain-language reason, shown behind "Show technical details".
    pub reason: String,
}

/// Parse `/proc/mounts` text. Lines that are not six fields are skipped; `\040`-style escapes in
/// the device and mount point are decoded.
pub fn parse_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split(' ').collect();
            // The last two fields are the dump and fsck-order numbers.
            let numbers = fields.len() == 6 && fields[4..].iter().all(|f| f.parse::<u32>().is_ok());
            if !numbers || fields[1].is_empty() {
                return None;
            }
            Some(Mount {
                device: unescape(fields[0]),
                mount_point: unescape(fields[1]),
                fs_type: fields[2].to_string(),
                options: fields[3].split(',').map(str::to_string).collect(),
            })
        })
        .collect()
}

/// The kernel writes space, tab, newline and backslash as `\040`, `\011`, `\012`, `\134`.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = (bytes[i] == b'\\' && i + 3 < bytes.len())
            .then(|| &bytes[i + 1..i + 4])
            .filter(|d| d.iter().all(|c| (b'0'..=b'7').contains(c)))
            .and_then(|d| {
                u8::try_from(d.iter().fold(0u32, |n, c| n * 8 + u32::from(c - b'0'))).ok()
            });
        match octal {
            Some(b) => {
                out.push(b);
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Can the access time of files under `path` be trusted? Looks for the mount with the longest
/// matching mount point.
pub fn access_time_for(mounts: &[Mount], path: &Path) -> AccessTimeReport {
    let Some(mount) = mounts
        .iter()
        .filter(|m| path.starts_with(&m.mount_point))
        .max_by_key(|m| m.mount_point.len())
    else {
        return AccessTimeReport {
            source: TimeSource::ModifiedTime,
            mount_point: String::new(),
            reason: "The drive's mount options could not be read, so modified time is used.".into(),
        };
    };
    let has = |opt: &str| mount.options.iter().any(|o| o == opt);
    // `relatime` (the default) still updates the access time when it is older than the modified
    // time or a day old, which is plenty for a six-month limit. `noatime` never updates it.
    let (source, reason) = if has("noatime") {
        (
            TimeSource::ModifiedTime,
            "This drive is mounted with noatime, so opening a file does not update its access time; modified time is used.",
        )
    } else {
        (
            TimeSource::AccessTime,
            "This drive records when files are opened (relatime or strictatime), so access time is used.",
        )
    };
    AccessTimeReport {
        source,
        mount_point: mount.mount_point.clone(),
        reason: reason.into(),
    }
}

/// The report for the home folder of this machine.
pub fn home_access_time() -> AccessTimeReport {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let home = std::env::var_os("HOME").unwrap_or_default();
    access_time_for(&parse_mounts(&mounts), Path::new(&home))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mounts");
        std::fs::read_to_string(path.join(name)).unwrap()
    }

    #[test]
    fn parses_a_real_arch_mounts_file() {
        let m = parse_mounts(&fixture("arch-relatime.txt"));
        assert_eq!(m.len(), 14);
        let home = m.iter().find(|m| m.mount_point == "/home").unwrap();
        assert_eq!(home.device, "/dev/nvme0n1p3");
        assert_eq!(home.fs_type, "ext4");
        assert_eq!(home.options, ["rw", "relatime"]);
    }

    #[test]
    fn decodes_escaped_spaces_in_mount_points() {
        let m = parse_mounts(&fixture("arch-relatime.txt"));
        assert!(m
            .iter()
            .any(|m| m.mount_point == "/run/media/ali/Mein Laufwerk"));
    }

    #[test]
    fn broken_lines_are_skipped_not_fatal() {
        let m = parse_mounts(&fixture("broken.txt"));
        assert_eq!(m.len(), 1, "only the one complete line: {m:?}");
        assert_eq!(m[0].mount_point, "/data");
        assert!(parse_mounts("").is_empty());
        assert!(parse_mounts("\0\0\0 \u{fffd}").is_empty());
    }

    #[test]
    fn relatime_is_good_enough_for_six_months() {
        let m = parse_mounts(&fixture("arch-relatime.txt"));
        let r = access_time_for(&m, Path::new("/home/ali/Downloads"));
        assert_eq!(r.source, TimeSource::AccessTime);
        assert_eq!(r.mount_point, "/home");
    }

    #[test]
    fn noatime_falls_back_to_modified_time() {
        let m = parse_mounts(&fixture("btrfs-noatime.txt"));
        let r = access_time_for(&m, Path::new("/home/ali"));
        assert_eq!(r.source, TimeSource::ModifiedTime);
        assert_eq!(r.mount_point, "/home");
        assert!(r.reason.contains("noatime"), "{}", r.reason);
    }

    #[test]
    fn the_longest_matching_mount_decides_including_unicode_paths() {
        let m = parse_mounts(&fixture("btrfs-noatime.txt"));
        let r = access_time_for(&m, Path::new("/home/ユーザー/データ/写真/a.jpg"));
        assert_eq!(r.mount_point, "/home/ユーザー/データ");
        assert_eq!(r.source, TimeSource::AccessTime);
    }

    #[test]
    fn a_mount_point_only_matches_whole_folder_names() {
        let m = parse_mounts(&fixture("btrfs-noatime.txt"));
        // "/home2" must not be treated as being under "/home".
        let r = access_time_for(&m, Path::new("/home2/x"));
        assert_eq!(r.mount_point, "/");
    }

    #[test]
    fn strictatime_is_reliable_and_unknown_mounts_fall_back() {
        let m = parse_mounts(&fixture("btrfs-noatime.txt"));
        let r = access_time_for(&m, Path::new("/mnt/Big Disk/movies"));
        assert_eq!(r.source, TimeSource::AccessTime);
        let none = access_time_for(&[], Path::new("/home/ali"));
        assert_eq!(none.source, TimeSource::ModifiedTime);
        assert!(none.mount_point.is_empty());
    }

    #[test]
    fn this_machine_gives_an_answer() {
        let r = home_access_time();
        assert!(!r.reason.is_empty());
    }
}
