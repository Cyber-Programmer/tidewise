//! Free space on the main drive, for the Home tile.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriveSpace {
    pub total: u64,
    pub free: u64,
}

impl DriveSpace {
    /// Share of the drive in use, 0.0–1.0.
    pub fn used_fraction(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        1.0 - self.free.min(self.total) as f64 / self.total as f64
    }
}

/// The drive mounted at `/`; if none is, the biggest one. Entries are (mount point, total, free).
pub fn pick_main(disks: &[(String, u64, u64)]) -> Option<DriveSpace> {
    let chosen = disks
        .iter()
        .find(|(mount, _, _)| mount == "/")
        .or_else(|| disks.iter().max_by_key(|(_, total, _)| *total))?;
    Some(DriveSpace {
        total: chosen.1,
        free: chosen.2,
    })
}

/// One mounted drive as the OS reports it, before filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDisk {
    pub name: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total: u64,
    pub available: u64,
}

/// A real drive to show on the Storage screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    pub name: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

/// Keep real drives: no pseudo filesystems (tmpfs, proc, squashfs of snaps, …), nothing of size 0,
/// each device once (btrfs subvolumes and bind mounts repeat it; the shortest mount point wins).
/// The drive at `/` comes first, then the biggest.
pub fn drives_from(_raw: &[RawDisk]) -> Vec<Drive> {
    Vec::new()
}

/// Every real drive on this machine.
pub fn drives() -> Vec<Drive> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let raw: Vec<_> = disks
        .list()
        .iter()
        .map(|d| RawDisk {
            name: d.name().to_string_lossy().into_owned(),
            mount_point: d.mount_point().to_string_lossy().into_owned(),
            fs_type: d.file_system().to_string_lossy().into_owned(),
            total: d.total_space(),
            available: d.available_space(),
        })
        .collect();
    drives_from(&raw)
}

/// Space on this machine's main drive, or None if no disk is reported.
pub fn main_drive() -> Option<DriveSpace> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let list: Vec<_> = disks
        .list()
        .iter()
        .map(|d| {
            (
                d.mount_point().to_string_lossy().into_owned(),
                d.total_space(),
                d.available_space(),
            )
        })
        .collect();
    pick_main(&list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(mount: &str, total: u64, free: u64) -> (String, u64, u64) {
        (mount.to_string(), total, free)
    }

    #[test]
    fn root_wins_over_bigger_disks() {
        let disks = [
            disk("/mnt/data", 4000, 3000),
            disk("/", 500, 100),
            disk("/boot", 1, 1),
        ];
        assert_eq!(
            pick_main(&disks),
            Some(DriveSpace {
                total: 500,
                free: 100
            })
        );
    }

    #[test]
    fn without_root_the_biggest_disk_is_used() {
        let disks = [disk("/a", 10, 5), disk("/b", 90, 20)];
        assert_eq!(
            pick_main(&disks),
            Some(DriveSpace {
                total: 90,
                free: 20
            })
        );
    }

    #[test]
    fn no_disks_is_none() {
        assert_eq!(pick_main(&[]), None);
    }

    #[test]
    fn used_fraction_handles_zero_and_overflow() {
        assert_eq!(DriveSpace { total: 0, free: 0 }.used_fraction(), 0.0);
        assert_eq!(
            DriveSpace {
                total: 100,
                free: 20
            }
            .used_fraction(),
            0.8
        );
        assert_eq!(
            DriveSpace {
                total: 100,
                free: 500
            }
            .used_fraction(),
            0.0
        );
    }

    fn raw(name: &str, mount: &str, fs: &str, total: u64, available: u64) -> RawDisk {
        RawDisk {
            name: name.into(),
            mount_point: mount.into(),
            fs_type: fs.into(),
            total,
            available,
        }
    }

    #[test]
    fn pseudo_filesystems_and_empty_drives_are_skipped() {
        let list = drives_from(&[
            raw("/dev/nvme0n1p2", "/", "ext4", 500, 100),
            raw("tmpfs", "/run", "tmpfs", 32, 30),
            raw("/dev/loop3", "/snap/core22/1380", "squashfs", 70, 0),
            raw("proc", "/proc", "proc", 0, 0),
            raw("efivarfs", "/sys/firmware/efi/efivars", "efivarfs", 1, 1),
            raw("/dev/sdc", "/mnt/empty", "ext4", 0, 0),
        ]);
        let mounts: Vec<_> = list.iter().map(|d| d.mount_point.as_str()).collect();
        assert_eq!(mounts, ["/"]);
    }

    #[test]
    fn used_is_total_minus_free_and_never_negative() {
        let list = drives_from(&[
            raw("/dev/a", "/", "ext4", 500, 100),
            raw("/dev/b", "/mnt/b", "ext4", 100, 500),
        ]);
        assert_eq!((list[0].total, list[0].used, list[0].free), (500, 400, 100));
        assert_eq!((list[1].used, list[1].free), (0, 100));
    }

    #[test]
    fn root_first_then_biggest_and_a_device_appears_once() {
        let list = drives_from(&[
            raw("/dev/sdb1", "/mnt/big", "ext4", 4000, 1000),
            raw("/dev/mapper/c", "/home", "btrfs", 900, 300),
            raw("/dev/mapper/c", "/", "btrfs", 900, 300),
            raw("/dev/sdc1", "/run/media/ali/Mein Laufwerk", "exfat", 64, 10),
        ]);
        let mounts: Vec<_> = list.iter().map(|d| d.mount_point.as_str()).collect();
        assert_eq!(mounts, ["/", "/mnt/big", "/run/media/ali/Mein Laufwerk"]);
    }

    #[test]
    fn this_machine_lists_its_main_drive() {
        let list = drives();
        assert!(!list.is_empty());
        assert!(list.iter().all(|d| d.total > 0 && d.used <= d.total));
    }

    #[test]
    fn this_machine_reports_a_sane_drive() {
        let d = main_drive().expect("a drive");
        assert!(d.total > 0 && d.free <= d.total);
    }
}
