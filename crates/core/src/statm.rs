//! `/proc/<pid>/statm`: resident and shared pages of one process.

/// Pages from `statm`. Only the fields we use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Statm {
    pub resident_pages: u64,
    pub shared_pages: u64,
}

/// Parse the contents of a `statm` file (`size resident shared text lib data dt`).
pub fn parse_statm(text: &str) -> Option<Statm> {
    let mut fields = text.split_whitespace().map(|f| f.parse::<u64>());
    let _size = fields.next()?.ok()?;
    let resident_pages = fields.next()?.ok()?;
    let shared_pages = fields.next()?.ok()?;
    Some(Statm {
        resident_pages,
        shared_pages,
    })
}

impl Statm {
    /// Memory only this process holds: (resident − shared) × page size.
    pub fn private_bytes(&self, page_size: u64) -> u64 {
        self.resident_pages
            .saturating_sub(self.shared_pages)
            .saturating_mul(page_size)
    }
}

/// Page size implied by a process's resident bytes (from sysinfo) and its resident pages.
/// Falls back to 4096 when the process has no resident pages to compare.
pub fn page_size_from(resident_bytes: u64, statm: &Statm) -> u64 {
    if statm.resident_pages > 0 && resident_bytes >= statm.resident_pages {
        resident_bytes / statm.resident_pages
    } else {
        4096
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = include_str!("../../../tests/fixtures/proc/statm-firefox.txt");
    const KERNEL: &str = include_str!("../../../tests/fixtures/proc/statm-kernel.txt");
    const BROKEN: &str = include_str!("../../../tests/fixtures/proc/statm-broken.txt");

    #[test]
    fn parses_real_statm() {
        let s = parse_statm(FIREFOX).unwrap();
        assert_eq!(s.resident_pages, 214_520);
        assert_eq!(s.shared_pages, 58_412);
    }

    #[test]
    fn private_is_resident_minus_shared_times_page_size() {
        let s = parse_statm(FIREFOX).unwrap();
        assert_eq!(s.private_bytes(4096), (214_520 - 58_412) * 4096);
    }

    #[test]
    fn shared_larger_than_resident_does_not_underflow() {
        let s = Statm {
            resident_pages: 10,
            shared_pages: 20,
        };
        assert_eq!(s.private_bytes(4096), 0);
    }

    #[test]
    fn kernel_threads_have_no_memory() {
        let s = parse_statm(KERNEL).unwrap();
        assert_eq!(s.private_bytes(4096), 0);
        assert_eq!(page_size_from(0, &s), 4096);
    }

    #[test]
    fn broken_or_empty_input_is_none() {
        assert_eq!(parse_statm(BROKEN), None);
        assert_eq!(parse_statm(""), None);
        assert_eq!(parse_statm("1 2"), None);
    }

    #[test]
    fn page_size_follows_sysinfo_resident_bytes() {
        let s = parse_statm(FIREFOX).unwrap();
        assert_eq!(page_size_from(214_520 * 16384, &s), 16384);
    }
}
