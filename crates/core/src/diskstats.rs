//! Cumulative disk counters from `/proc/diskstats`.

/// Total bytes read and written since boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiskCounters {
    pub read_bytes: u64,
    pub write_bytes: u64,
}

const SECTOR_BYTES: u64 = 512;

/// Sum the counters of whole disks. `is_whole_disk` says which device names count,
/// so partitions, loop devices and RAM disks are not counted twice.
pub fn parse_diskstats(text: &str, is_whole_disk: impl Fn(&str) -> bool) -> DiskCounters {
    let mut total = DiskCounters::default();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(name), Some(read), Some(write)) = (fields.get(2), fields.get(5), fields.get(9))
        else {
            continue;
        };
        let (Ok(read), Ok(write)) = (read.parse::<u64>(), write.parse::<u64>()) else {
            continue;
        };
        if is_whole_disk(name) {
            total.read_bytes = total
                .read_bytes
                .saturating_add(read.saturating_mul(SECTOR_BYTES));
            total.write_bytes = total
                .write_bytes
                .saturating_add(write.saturating_mul(SECTOR_BYTES));
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/proc/diskstats");

    fn physical(name: &str) -> bool {
        name == "sda" || name == "nvme0n1"
    }

    #[test]
    fn sums_only_whole_disks() {
        let c = parse_diskstats(FIXTURE, physical);
        assert_eq!(c.read_bytes, (2000 + 10000) * 512);
        assert_eq!(c.write_bytes, (4000 + 8000) * 512);
    }

    #[test]
    fn empty_input_is_zero() {
        assert_eq!(parse_diskstats("", physical), DiskCounters::default());
    }

    #[test]
    fn garbage_lines_are_skipped() {
        let c = parse_diskstats("not a diskstats line\n8 0 sda x y z\n", |_| true);
        assert_eq!(c, DiskCounters::default());
    }
}
