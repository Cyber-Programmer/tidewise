//! Size of the systemd journal, from `journalctl --disk-usage` (readable without root).

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("no size like `2.8G` found in the journalctl output")]
    NoSize,
}

/// Bytes the journals take, from a line like
/// `Archived and active journals take up 2.8G in the file system.`
pub fn parse_disk_usage(text: &str) -> Result<u64, ParseError> {
    let _ = text;
    Err(ParseError::NoSize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/junk/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn real_output_is_in_binary_units() {
        // 2.8G = 2.8 * 1024^3 (journalctl uses powers of 1024).
        assert_eq!(
            parse_disk_usage(&fixture("journal-real.txt")),
            Ok(3_006_477_107)
        );
    }

    #[test]
    fn german_wording_and_comma_decimal() {
        assert_eq!(
            parse_disk_usage(&fixture("journal-de.txt")),
            Ok(1_610_612_736)
        );
    }

    #[test]
    fn zero_bytes() {
        assert_eq!(parse_disk_usage(&fixture("journal-zero.txt")), Ok(0));
    }

    #[test]
    fn broken_or_empty_output_is_an_error() {
        assert_eq!(
            parse_disk_usage(&fixture("journal-broken.txt")),
            Err(ParseError::NoSize)
        );
        assert_eq!(parse_disk_usage(""), Err(ParseError::NoSize));
        assert_eq!(parse_disk_usage("take up 5X in"), Err(ParseError::NoSize));
        assert_eq!(
            parse_disk_usage("take up 99999999999999999999T"),
            Err(ParseError::NoSize)
        );
    }
}
