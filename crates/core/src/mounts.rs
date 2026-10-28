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
pub fn parse_mounts(_text: &str) -> Vec<Mount> {
    Vec::new()
}

/// Can the access time of files under `path` be trusted? Looks for the mount with the longest
/// matching mount point.
pub fn access_time_for(_mounts: &[Mount], _path: &Path) -> AccessTimeReport {
    AccessTimeReport {
        source: TimeSource::AccessTime,
        mount_point: String::new(),
        reason: String::new(),
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
