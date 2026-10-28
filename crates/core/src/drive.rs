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

    #[test]
    fn this_machine_reports_a_sane_drive() {
        let d = main_drive().expect("a drive");
        assert!(d.total > 0 && d.free <= d.total);
    }
}
